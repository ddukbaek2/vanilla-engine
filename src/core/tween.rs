//==============================================================================
// 트윈.
// - 원본 core/tween.js 를 옮긴 것이다. 시작값 묶음(이름 → 숫자 또는 Vector2)과 목표값 묶음 사이를
//   대기(delay) 뒤 duration 초 동안 이징 함수로 보간하고, tick 마다 보간한 값 묶음을 갱신 콜백에 넘긴다.
//   다 가면 멈추고 완료 콜백을 부른다. 반복이나 요요는 원본에 없어서 옮기지 않았다.
// - 원본의 Tween.easingFunction.back.out 은 easingFunction::back::out 이다.
//   in 은 러스트 예약어라 easingFunction::quadratic::r#in 처럼 r# 을 붙여 부른다. (이름은 원본 그대로)
// - 원본의 콜백은 노드 객체를 붙잡고 node.setPosition(...) 을 부른다. 러스트 판은 노드가 씬의 NodeTree 에 있어서
//   갱신 콜백과 완료 콜백이 &mut NodeTree 를 함께 받는다. (붙잡아 둔 NodeId 로 노드를 꺼내 쓴다)
// - 러스트 판에서 덧붙인 것: setTarget(nodeId). 값 이름이 노드 속성 이름(TweenProperty, 예: "localPosition")이면
//   tick 마다 그 노드에 바로 쓴다. 대상 노드가 지워지면 콜백 없이 조용히 끝난다. createNodeTween 은 그것의 줄임이다.
// - 원본은 이징 계산에 base/math.js 의 pow 를 쓰는데, 그 pow 는 정수 지수만 맞게 계산한다.
//   그래서 원본의 exponential, elastic 은 공식과 다른 값을 낸다. (예: exponential.in 은 0 보다 크면 늘 1)
//   러스트 판은 표준 powf 로 공식대로 계산한다. 나머지 이징은 원본과 값이 같다.
//   (node 로 원본을 돌려 진행률 1001 곳을 대조했다. 나머지의 차이는 1e-8 아래)
// - duration 이 0 이하이면 원본은 0 으로 나눠 한 프레임 NaN 이 나올 수 있다. 러스트 판은 바로 끝(진행률 1)으로 친다.
// - 씬은 SceneBase::startTween 으로 트윈을 넘겨받아 들고, 노드 tick → 터치 다음에 돌리며 끝난 트윈을 뺀다.
//   원본은 트윈 객체를 들고 stopTween(tween) 으로 가리키지만, 러스트 판은 startTween 이 돌려준 TweenId 로 가리킨다.
//==============================================================================
use crate::base::identifier;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::WorldNode;
use std::borrow::Cow;

//==============================================================================
// 트윈 값. (원본은 숫자와 Vector2 만 보간한다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TweenValue {
	number(f32),
	vector2(Vector2),
}

impl From<f32> for TweenValue {
	//==============================================================================
	// 숫자 → 트윈 값.
	//==============================================================================
	fn from(number: f32) -> TweenValue {
		TweenValue::number(number)
	}
}

impl From<Vector2> for TweenValue {
	//==============================================================================
	// Vector2 → 트윈 값.
	//==============================================================================
	fn from(vector: Vector2) -> TweenValue {
		TweenValue::vector2(vector)
	}
}

//==============================================================================
// 이름 붙은 트윈 값 하나.
//==============================================================================
#[derive(Clone, Debug)]
struct TweenValueEntry {
	name: Cow<'static, str>,
	value: TweenValue,
}

//==============================================================================
// 트윈 값 묶음. (원본의 { x: 0, y: 0 } 같은 객체 자리, 넣은 순서를 지킨다)
// - 이름은 보통 문자열 상수("x")를 쓴다. 상수 이름은 복사할 때 메모리를 새로 잡지 않는다.
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct TweenValues {
	entries: Vec<TweenValueEntry>,
}

impl TweenValues {
	//==============================================================================
	// 생성. (비어 있다)
	//==============================================================================
	pub fn new() -> TweenValues {
		TweenValues {
			entries: Vec::new(),
		}
	}

	//==============================================================================
	// 값을 넣고 자신을 돌려준다. (이어 쓰기용, 같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn with(mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TweenValue>) -> TweenValues {
		self.set(name, value);
		self
	}

	//==============================================================================
	// 값 설정. (같은 이름이 있으면 바꾸고, 없으면 뒤에 붙인다)
	//==============================================================================
	pub fn set(&mut self, name: impl Into<Cow<'static, str>>, value: impl Into<TweenValue>) {
		let name = name.into();
		let value = value.into();
		for entry in self.entries.iter_mut() {
			if entry.name == name {
				entry.value = value;
				return;
			}
		}
		self.entries.push(TweenValueEntry {
			name,
			value,
		});
	}

	//==============================================================================
	// 값 반환. (없으면 None)
	//==============================================================================
	pub fn get(&self, name: &str) -> Option<TweenValue> {
		self.entries.iter().find(|entry| entry.name == name).map(|entry| entry.value)
	}

	//==============================================================================
	// 숫자 값 반환. (없거나 숫자가 아니면 None)
	//==============================================================================
	pub fn getNumber(&self, name: &str) -> Option<f32> {
		match self.get(name) {
			Some(TweenValue::number(number)) => {
				Some(number)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// Vector2 값 반환. (없거나 Vector2 가 아니면 None)
	//==============================================================================
	pub fn getVector2(&self, name: &str) -> Option<Vector2> {
		match self.get(name) {
			Some(TweenValue::vector2(vector)) => {
				Some(vector)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 값 수.
	//==============================================================================
	pub fn getCount(&self) -> usize {
		self.entries.len()
	}

	//==============================================================================
	// 모두 비운다. (잡아 둔 메모리는 남긴다)
	//==============================================================================
	fn clear(&mut self) {
		self.entries.clear();
	}

	//==============================================================================
	// 같은 이름을 찾지 않고 뒤에 붙인다. (tick 이 보간 결과를 채울 때)
	//==============================================================================
	fn push(&mut self, name: Cow<'static, str>, value: TweenValue) {
		self.entries.push(TweenValueEntry {
			name,
			value,
		});
	}
}

//==============================================================================
// 트윈이 바로 쓰는 노드 속성. (setTarget 으로 대상 노드를 정했을 때, 값 이름이 이 이름이면 그 노드에 쓴다)
// - Vector2 속성에 숫자를 주면 두 축에 같이 쓴다. (원본 예제의 setScale(Vector2.one().multiply(v.scale)) 자리)
// - 숫자 속성에 Vector2 를 주면 쓰지 않는다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TweenProperty {
	localPosition,
	localScale,
	localRotation,
	localOpacity,
	contentSize,
}

impl TweenProperty {
	//==============================================================================
	// 값 묶음에서 쓰는 이름. (enum 이름과 같다)
	//==============================================================================
	pub fn getName(&self) -> &'static str {
		match self {
			TweenProperty::localPosition => {
				"localPosition"
			}
			TweenProperty::localScale => {
				"localScale"
			}
			TweenProperty::localRotation => {
				"localRotation"
			}
			TweenProperty::localOpacity => {
				"localOpacity"
			}
			TweenProperty::contentSize => {
				"contentSize"
			}
		}
	}

	//==============================================================================
	// 이름으로 찾기. (노드 속성 이름이 아니면 None)
	//==============================================================================
	pub fn findByName(name: &str) -> Option<TweenProperty> {
		match name {
			"localPosition" => {
				Some(TweenProperty::localPosition)
			}
			"localScale" => {
				Some(TweenProperty::localScale)
			}
			"localRotation" => {
				Some(TweenProperty::localRotation)
			}
			"localOpacity" => {
				Some(TweenProperty::localOpacity)
			}
			"contentSize" => {
				Some(TweenProperty::contentSize)
			}
			_ => {
				None
			}
		}
	}

	//==============================================================================
	// 노드에 값 쓰기.
	//==============================================================================
	pub fn applyToNode(&self, node: &mut WorldNode, value: TweenValue) {
		let vector = match value {
			TweenValue::number(number) => {
				Vector2::create(number, number)
			}
			TweenValue::vector2(vector) => {
				vector
			}
		};
		match self {
			TweenProperty::localPosition => {
				node.setLocalPosition(vector);
			}
			TweenProperty::localScale => {
				node.setLocalScale(vector);
			}
			TweenProperty::contentSize => {
				node.setContentSize(vector);
			}
			TweenProperty::localRotation => {
				if let TweenValue::number(number) = value {
					node.setLocalRotation(number);
				}
			}
			TweenProperty::localOpacity => {
				if let TweenValue::number(number) = value {
					node.setLocalOpacity(number);
				}
			}
		}
	}
}

//==============================================================================
// 트윈 핸들. (SceneBase::startTween 이 돌려주고, stopTween / getTween 에 넘긴다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TweenId(u64);

// 갱신 콜백. (보간한 값 묶음과 노드 트리를 받는다)
type TweenUpdateCallback = Box<dyn FnMut(&TweenValues, &mut NodeTree)>;

// 완료 콜백. (노드 트리를 받는다)
type TweenCompleteCallback = Box<dyn FnMut(&mut NodeTree)>;

//==============================================================================
// 트윈.
//==============================================================================
pub struct Tween {
	tweenId: TweenId,
	valuesStart: TweenValues,
	valuesEnd: TweenValues,
	currentValues: TweenValues,
	duration: f32,
	delay: f32,
	elapsedTime: f32,
	easingFunction: Box<dyn Fn(f32) -> f32>,
	tickCallback: Option<TweenUpdateCallback>,
	completeCallback: Option<TweenCompleteCallback>,
	targetNodeId: Option<NodeId>,
	isPlaying: bool,
	isFinished: bool,
}

impl Tween {
	//==============================================================================
	// 생성. (시작값 묶음을 받는다. 길이 1초, 대기 0, linear)
	//==============================================================================
	pub fn new(initialValues: TweenValues) -> Tween {
		let instanceId = identifier::createInstanceId();
		Tween {
			tweenId: TweenId(instanceId),
			valuesStart: initialValues,
			valuesEnd: TweenValues::new(),
			currentValues: TweenValues::new(),
			duration: 1.0,
			delay: 0.0,
			elapsedTime: 0.0,
			easingFunction: Box::new(easingFunction::linear),
			tickCallback: None,
			completeCallback: None,
			targetNodeId: None,
			isPlaying: false,
			isFinished: false,
		}
	}

	//==============================================================================
	// 노드 속성 하나를 움직이는 트윈 생성. (러스트 판에서 덧붙인 줄임)
	// - Tween::new(시작값).to(목표값, duration).setTarget(nodeId) 와 같다. 값 이름은 속성 이름이다.
	//==============================================================================
	pub fn createNodeTween(nodeId: NodeId, tweenProperty: TweenProperty, startValue: impl Into<TweenValue>, endValue: impl Into<TweenValue>, duration: f32) -> Tween {
		let propertyName = tweenProperty.getName();
		let initialValues = TweenValues::new().with(propertyName, startValue);
		let targetValues = TweenValues::new().with(propertyName, endValue);
		Tween::new(initialValues).to(targetValues, duration).setTarget(nodeId)
	}

	//==============================================================================
	// 갱신.
	// - 대기 시간이 지나면 진행률(0 ~ 1)에 이징을 걸어 목표값 이름 순서로 보간한다.
	//   시작값에 없는 이름, 시작값과 종류(숫자 / Vector2)가 다른 이름은 건너뛴다. (원본과 같음)
	// - 대상 노드가 있으면 노드 속성 이름의 값을 그 노드에 쓰고, 갱신 콜백을 부른다.
	// - 진행률이 1 이 되면 멈추고 완료 콜백을 부른다.
	//==============================================================================
	pub fn tick(&mut self, nodeTree: &mut NodeTree, timeDelta: f32) {
		if !self.isPlaying || self.isFinished {
			return;
		}

		// 대상 노드가 지워졌으면 콜백 없이 끝낸다. (러스트 판에서 덧붙인 것)
		if let Some(targetNodeId) = self.targetNodeId {
			if !nodeTree.contains(targetNodeId) {
				self.stop();
				return;
			}
		}

		self.elapsedTime += timeDelta;
		if self.elapsedTime < self.delay {
			return;
		}

		let progress = if self.duration > 0.0 {
			let rawProgress = (self.elapsedTime - self.delay) / self.duration;
			math::clamp(rawProgress, 0.0, 1.0)
		}
		else {
			1.0
		};
		let easedProgress = (self.easingFunction)(progress);

		// 보간.
		self.currentValues.clear();
		for endEntry in self.valuesEnd.entries.iter() {
			let startValue = match self.valuesStart.get(&endEntry.name) {
				Some(startValue) => {
					startValue
				}
				None => {
					continue;
				}
			};
			match (startValue, endEntry.value) {
				// 일반 숫자.
				(TweenValue::number(startNumber), TweenValue::number(endNumber)) => {
					let number = math::lerp(startNumber, endNumber, easedProgress);
					self.currentValues.push(endEntry.name.clone(), TweenValue::number(number));
				}
				// 2차원 벡터.
				(TweenValue::vector2(startVector), TweenValue::vector2(endVector)) => {
					let interpolatedX = math::lerp(startVector.x, endVector.x, easedProgress);
					let interpolatedY = math::lerp(startVector.y, endVector.y, easedProgress);
					let interpolatedVector = Vector2::create(interpolatedX, interpolatedY);
					self.currentValues.push(endEntry.name.clone(), TweenValue::vector2(interpolatedVector));
				}
				_ => {
				}
			}
		}

		// 대상 노드에 쓰기.
		if let Some(targetNodeId) = self.targetNodeId {
			if let Some(targetNode) = nodeTree.getNodeMut(targetNodeId) {
				for entry in self.currentValues.entries.iter() {
					if let Some(tweenProperty) = TweenProperty::findByName(&entry.name) {
						tweenProperty.applyToNode(targetNode, entry.value);
					}
				}
			}
		}

		if let Some(tickCallback) = self.tickCallback.as_mut() {
			tickCallback(&self.currentValues, nodeTree);
		}

		if progress >= 1.0 {
			self.stop();
			if let Some(completeCallback) = self.completeCallback.as_mut() {
				completeCallback(nodeTree);
			}
		}
	}

	//==============================================================================
	// 목표값 설정. (길이도 함께 정한다)
	//==============================================================================
	pub fn to(mut self, properties: TweenValues, duration: f32) -> Tween {
		self.valuesEnd = properties;
		self.duration = duration;
		self
	}

	//==============================================================================
	// 시작 전 대기 시간 설정. (초)
	//==============================================================================
	pub fn delay(mut self, amount: f32) -> Tween {
		self.delay = amount;
		self
	}

	//==============================================================================
	// 이징 함수 설정. (easingFunction 모듈의 함수, 또는 진행률을 받아 비율을 돌려주는 아무 함수)
	//==============================================================================
	pub fn easing(mut self, easingFunction: impl Fn(f32) -> f32 + 'static) -> Tween {
		self.easingFunction = Box::new(easingFunction);
		self
	}

	//==============================================================================
	// 갱신 콜백 설정. (보간한 값 묶음과 노드 트리를 받는다)
	//==============================================================================
	pub fn setUpdate(mut self, callback: impl FnMut(&TweenValues, &mut NodeTree) + 'static) -> Tween {
		self.tickCallback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	// 완료 콜백 설정. (노드 트리를 받는다)
	//==============================================================================
	pub fn setComplete(mut self, callback: impl FnMut(&mut NodeTree) + 'static) -> Tween {
		self.completeCallback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	// 대상 노드 설정. (러스트 판에서 덧붙인 것. 값 이름이 TweenProperty 이름이면 이 노드에 쓴다)
	//==============================================================================
	pub fn setTarget(mut self, nodeId: NodeId) -> Tween {
		self.targetNodeId = Some(nodeId);
		self
	}

	//==============================================================================
	// 시작. (이미 재생 중이면 아무것도 하지 않는다)
	//==============================================================================
	pub fn start(&mut self) {
		if self.isPlaying {
			return;
		}

		self.isPlaying = true;
		self.isFinished = false;
		self.elapsedTime = 0.0;
	}

	//==============================================================================
	// 정지. (완료 콜백은 부르지 않는다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.isPlaying = false;
		self.isFinished = true;
	}

	//==============================================================================
	// 완료 여부 반환.
	//==============================================================================
	pub fn isFinished(&self) -> bool {
		self.isFinished
	}

	//==============================================================================
	// 핸들 반환.
	//==============================================================================
	pub fn getTweenId(&self) -> TweenId {
		self.tweenId
	}

	//==============================================================================
	// 대상 노드 핸들 반환. (setTarget 을 하지 않았으면 None)
	//==============================================================================
	pub fn getTargetNodeId(&self) -> Option<NodeId> {
		self.targetNodeId
	}
}

//==============================================================================
// 이징 함수. (원본 Tween.easingFunction)
// - 진행률(0 ~ 1)을 받아 보간 비율을 돌려준다. back, elastic 은 0 ~ 1 밖으로 넘친다.
// - 원본은 인자 k 를 그 자리에서 바꿔 가며(k *= 2, --k) 쓴다. 여기서는 바뀐 값마다 이름을 붙였다.
//==============================================================================
pub mod easingFunction {
	use crate::base::math::PI;

	//==============================================================================
	// 선형.
	//==============================================================================
	pub fn linear(progress: f32) -> f32 {
		progress
	}

	//==============================================================================
	// 2차 곡선.
	//==============================================================================
	pub mod quadratic {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			progress * (2.0 - progress)
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * doubledProgress * doubledProgress;
			}
			let shiftedProgress = doubledProgress - 1.0;
			-0.5 * (shiftedProgress * (shiftedProgress - 2.0) - 1.0)
		}
	}

	//==============================================================================
	// 3차 곡선.
	//==============================================================================
	pub mod cubic {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shiftedProgress = progress - 1.0;
			shiftedProgress * shiftedProgress * shiftedProgress + 1.0
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * doubledProgress * doubledProgress * doubledProgress;
			}
			let shiftedProgress = doubledProgress - 2.0;
			0.5 * (shiftedProgress * shiftedProgress * shiftedProgress + 2.0)
		}
	}

	//==============================================================================
	// 4차 곡선.
	//==============================================================================
	pub mod quartic {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress * progress
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shiftedProgress = progress - 1.0;
			1.0 - (shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress)
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * doubledProgress * doubledProgress * doubledProgress * doubledProgress;
			}
			let shiftedProgress = doubledProgress - 2.0;
			-0.5 * (shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress - 2.0)
		}
	}

	//==============================================================================
	// 5차 곡선.
	//==============================================================================
	pub mod quintic {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * progress * progress * progress
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shiftedProgress = progress - 1.0;
			shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress + 1.0
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * doubledProgress * doubledProgress * doubledProgress * doubledProgress * doubledProgress;
			}
			let shiftedProgress = doubledProgress - 2.0;
			0.5 * (shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress * shiftedProgress + 2.0)
		}
	}

	//==============================================================================
	// 사인 곡선.
	//==============================================================================
	pub mod sinusoidal {
		use super::PI;

		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - (progress * PI / 2.0).cos()
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			(progress * PI / 2.0).sin()
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			0.5 * (1.0 - (PI * progress).cos())
		}
	}

	//==============================================================================
	// 지수 곡선. (원본은 정수 지수만 맞는 pow 를 써서 값이 틀린다. 여기서는 공식대로 powf)
	//==============================================================================
	pub mod exponential {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			1024.0f32.powf(progress - 1.0)
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress == 1.0 {
				return 1.0;
			}
			1.0 - 2.0f32.powf(-10.0 * progress)
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * 1024.0f32.powf(doubledProgress - 1.0);
			}
			0.5 * (-2.0f32.powf(-10.0 * (doubledProgress - 1.0)) + 2.0)
		}
	}

	//==============================================================================
	// 원 곡선.
	//==============================================================================
	pub mod circular {
		//==============================================================================
		// 천천히 시작한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - (1.0 - progress * progress).sqrt()
		}

		//==============================================================================
		// 천천히 끝난다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shiftedProgress = progress - 1.0;
			(1.0 - (shiftedProgress * shiftedProgress)).sqrt()
		}

		//==============================================================================
		// 천천히 시작하고 천천히 끝난다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return -0.5 * ((1.0 - doubledProgress * doubledProgress).sqrt() - 1.0);
			}
			let shiftedProgress = doubledProgress - 2.0;
			0.5 * ((1.0 - shiftedProgress * shiftedProgress).sqrt() + 1.0)
		}
	}

	//==============================================================================
	// 튕기는 고무줄.
	// - 원본은 a = 0.1, p = 0.4 로 시작하지만 a < 1 이면 a = 1, s = p / 4 로 바꾸므로 늘 아래 값이다.
	// - 원본은 정수 지수만 맞는 pow 를 써서 값이 틀린다. 여기서는 공식대로 powf.
	//==============================================================================
	pub mod elastic {
		use super::PI;

		const ELASTIC_AMPLITUDE: f32 = 1.0;
		const ELASTIC_PERIOD: f32 = 0.4;
		const ELASTIC_SHIFT: f32 = ELASTIC_PERIOD / 4.0;

		//==============================================================================
		// 시작에서 떨다가 나아간다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let shiftedProgress = progress - 1.0;
			let wave = ((shiftedProgress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			-(ELASTIC_AMPLITUDE * 2.0f32.powf(10.0 * shiftedProgress) * wave)
		}

		//==============================================================================
		// 끝을 넘었다가 떨며 멈춘다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let wave = ((progress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			ELASTIC_AMPLITUDE * 2.0f32.powf(-10.0 * progress) * wave + 1.0
		}

		//==============================================================================
		// 시작과 끝에서 떤다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			if progress == 0.0 {
				return 0.0;
			}
			if progress == 1.0 {
				return 1.0;
			}
			let doubledProgress = progress * 2.0;
			let shiftedProgress = doubledProgress - 1.0;
			let wave = ((shiftedProgress - ELASTIC_SHIFT) * (2.0 * PI) / ELASTIC_PERIOD).sin();
			if doubledProgress < 1.0 {
				return -0.5 * (ELASTIC_AMPLITUDE * 2.0f32.powf(10.0 * shiftedProgress) * wave);
			}
			ELASTIC_AMPLITUDE * 2.0f32.powf(-10.0 * shiftedProgress) * wave * 0.5 + 1.0
		}
	}

	//==============================================================================
	// 뒤로 살짝 물러났다가 간다.
	//==============================================================================
	pub mod back {
		const BACK_OVERSHOOT: f32 = 1.70158;
		const BACK_IN_OUT_OVERSHOOT: f32 = 1.70158 * 1.525;

		//==============================================================================
		// 뒤로 물러났다가 출발한다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			progress * progress * ((BACK_OVERSHOOT + 1.0) * progress - BACK_OVERSHOOT)
		}

		//==============================================================================
		// 끝을 살짝 넘었다가 돌아온다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			let shiftedProgress = progress - 1.0;
			shiftedProgress * shiftedProgress * ((BACK_OVERSHOOT + 1.0) * shiftedProgress + BACK_OVERSHOOT) + 1.0
		}

		//==============================================================================
		// 시작에서 물러나고 끝에서 넘친다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			let doubledProgress = progress * 2.0;
			if doubledProgress < 1.0 {
				return 0.5 * (doubledProgress * doubledProgress * ((BACK_IN_OUT_OVERSHOOT + 1.0) * doubledProgress - BACK_IN_OUT_OVERSHOOT));
			}
			let shiftedProgress = doubledProgress - 2.0;
			0.5 * (shiftedProgress * shiftedProgress * ((BACK_IN_OUT_OVERSHOOT + 1.0) * shiftedProgress + BACK_IN_OUT_OVERSHOOT) + 2.0)
		}
	}

	//==============================================================================
	// 바닥에 튕긴다.
	//==============================================================================
	pub mod bounce {
		//==============================================================================
		// 시작에서 튕기다 나아간다.
		//==============================================================================
		pub fn r#in(progress: f32) -> f32 {
			1.0 - out(1.0 - progress)
		}

		//==============================================================================
		// 끝에 닿아 튕기다 멈춘다.
		//==============================================================================
		pub fn out(progress: f32) -> f32 {
			if progress < 1.0 / 2.75 {
				7.5625 * progress * progress
			}
			else if progress < 2.0 / 2.75 {
				let shiftedProgress = progress - 1.5 / 2.75;
				7.5625 * shiftedProgress * shiftedProgress + 0.75
			}
			else if progress < 2.5 / 2.75 {
				let shiftedProgress = progress - 2.25 / 2.75;
				7.5625 * shiftedProgress * shiftedProgress + 0.9375
			}
			else {
				let shiftedProgress = progress - 2.625 / 2.75;
				7.5625 * shiftedProgress * shiftedProgress + 0.984375
			}
		}

		//==============================================================================
		// 시작과 끝에서 튕긴다.
		//==============================================================================
		pub fn inOut(progress: f32) -> f32 {
			if progress < 0.5 {
				return r#in(progress * 2.0) * 0.5;
			}
			out(progress * 2.0 - 1.0) * 0.5 + 0.5
		}
	}
}

//==============================================================================
// 시험. (GL 없이 돈다. NodeTree::new 와 Engine::new 는 창이나 GL 이 없어도 된다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::ComponentContext;
	use crate::core::scene::SceneBase;
	use std::cell::{Cell, RefCell};
	use std::rc::Rc;

	const EASING_SAMPLE_PROGRESSES: [f32; 7] = [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];

	// 이징 기준표 한 줄. (이름, 함수, 진행률 일곱 곳의 값)
	type EasingReferenceRow = (&'static str, fn(f32) -> f32, [f64; 7]);

	//==============================================================================
	// 이징 기준표.
	// - 원본 tween.js 의 Tween.easingFunction 을 node 에서 그대로 불러, Math 만 표준 Math 로 바꿔 뽑은 값이다.
	//   (원본 그대로 돌리면 base/math.js 의 pow 때문에 exponential, elastic 만 다르다. 나머지는 원본 그대로와 같다)
	// - 뽑은 숫자를 그대로 적어서 1/√2 같은 상수와 닮은 값이 있다. (approx_constant 를 끈다)
	//==============================================================================
	#[allow(clippy::approx_constant)]
	fn getEasingReferenceTable() -> Vec<EasingReferenceRow> {
		vec![
			("linear", easingFunction::linear as fn(f32) -> f32, [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0]),
			("quadratic.in", easingFunction::quadratic::r#in as fn(f32) -> f32, [0.000000000, 0.010000000, 0.062500000, 0.250000000, 0.562500000, 0.810000000, 1.000000000]),
			("quadratic.out", easingFunction::quadratic::out as fn(f32) -> f32, [0.000000000, 0.190000000, 0.437500000, 0.750000000, 0.937500000, 0.990000000, 1.000000000]),
			("quadratic.inOut", easingFunction::quadratic::inOut as fn(f32) -> f32, [0.000000000, 0.020000000, 0.125000000, 0.500000000, 0.875000000, 0.980000000, 1.000000000]),
			("cubic.in", easingFunction::cubic::r#in as fn(f32) -> f32, [0.000000000, 0.001000000, 0.015625000, 0.125000000, 0.421875000, 0.729000000, 1.000000000]),
			("cubic.out", easingFunction::cubic::out as fn(f32) -> f32, [0.000000000, 0.271000000, 0.578125000, 0.875000000, 0.984375000, 0.999000000, 1.000000000]),
			("cubic.inOut", easingFunction::cubic::inOut as fn(f32) -> f32, [0.000000000, 0.004000000, 0.062500000, 0.500000000, 0.937500000, 0.996000000, 1.000000000]),
			("quartic.in", easingFunction::quartic::r#in as fn(f32) -> f32, [0.000000000, 0.000100000, 0.003906250, 0.062500000, 0.316406250, 0.656100000, 1.000000000]),
			("quartic.out", easingFunction::quartic::out as fn(f32) -> f32, [0.000000000, 0.343900000, 0.683593750, 0.937500000, 0.996093750, 0.999900000, 1.000000000]),
			("quartic.inOut", easingFunction::quartic::inOut as fn(f32) -> f32, [0.000000000, 0.000800000, 0.031250000, 0.500000000, 0.968750000, 0.999200000, 1.000000000]),
			("quintic.in", easingFunction::quintic::r#in as fn(f32) -> f32, [0.000000000, 0.000010000, 0.000976563, 0.031250000, 0.237304688, 0.590490000, 1.000000000]),
			("quintic.out", easingFunction::quintic::out as fn(f32) -> f32, [0.000000000, 0.409510000, 0.762695313, 0.968750000, 0.999023438, 0.999990000, 1.000000000]),
			("quintic.inOut", easingFunction::quintic::inOut as fn(f32) -> f32, [0.000000000, 0.000160000, 0.015625000, 0.500000000, 0.984375000, 0.999840000, 1.000000000]),
			("sinusoidal.in", easingFunction::sinusoidal::r#in as fn(f32) -> f32, [0.000000000, 0.012311659, 0.076120467, 0.292893219, 0.617316568, 0.843565535, 1.000000000]),
			("sinusoidal.out", easingFunction::sinusoidal::out as fn(f32) -> f32, [0.000000000, 0.156434465, 0.382683432, 0.707106781, 0.923879533, 0.987688341, 1.000000000]),
			("sinusoidal.inOut", easingFunction::sinusoidal::inOut as fn(f32) -> f32, [0.000000000, 0.024471742, 0.146446609, 0.500000000, 0.853553391, 0.975528258, 1.000000000]),
			("exponential.in", easingFunction::exponential::r#in as fn(f32) -> f32, [0.000000000, 0.001953125, 0.005524272, 0.031250000, 0.176776695, 0.500000000, 1.000000000]),
			("exponential.out", easingFunction::exponential::out as fn(f32) -> f32, [0.000000000, 0.500000000, 0.823223305, 0.968750000, 0.994475728, 0.998046875, 1.000000000]),
			("exponential.inOut", easingFunction::exponential::inOut as fn(f32) -> f32, [0.000000000, 0.001953125, 0.015625000, 0.500000000, 0.984375000, 0.998046875, 1.000000000]),
			("circular.in", easingFunction::circular::r#in as fn(f32) -> f32, [0.000000000, 0.005012563, 0.031754163, 0.133974596, 0.338562172, 0.564110106, 1.000000000]),
			("circular.out", easingFunction::circular::out as fn(f32) -> f32, [0.000000000, 0.435889894, 0.661437828, 0.866025404, 0.968245837, 0.994987437, 1.000000000]),
			("circular.inOut", easingFunction::circular::inOut as fn(f32) -> f32, [0.000000000, 0.010102051, 0.066987298, 0.500000000, 0.933012702, 0.989897949, 1.000000000]),
			("elastic.in", easingFunction::elastic::r#in as fn(f32) -> f32, [0.000000000, 0.000000000, 0.003906250, 0.000000000, -0.125000000, 0.000000000, 1.000000000]),
			("elastic.out", easingFunction::elastic::out as fn(f32) -> f32, [0.000000000, 1.000000000, 1.125000000, 1.000000000, 0.996093750, 1.000000000, 1.000000000]),
			("elastic.inOut", easingFunction::elastic::inOut as fn(f32) -> f32, [0.000000000, 0.001953125, 0.000000000, 0.500000000, 1.000000000, 0.998046875, 1.000000000]),
			("back.in", easingFunction::back::r#in as fn(f32) -> f32, [0.000000000, -0.014314220, -0.064136563, -0.087697500, 0.182590312, 0.591172020, 1.000000000]),
			("back.out", easingFunction::back::out as fn(f32) -> f32, [0.000000000, 0.408827980, 0.817409688, 1.087697500, 1.064136563, 1.014314220, 1.000000000]),
			("back.inOut", easingFunction::back::inOut as fn(f32) -> f32, [0.000000000, -0.037518552, -0.099681844, 0.500000000, 1.099681844, 1.037518552, 1.000000000]),
			("bounce.in", easingFunction::bounce::r#in as fn(f32) -> f32, [0.000000000, 0.011875000, 0.027343750, 0.234375000, 0.527343750, 0.924375000, 1.000000000]),
			("bounce.out", easingFunction::bounce::out as fn(f32) -> f32, [0.000000000, 0.075625000, 0.472656250, 0.765625000, 0.972656250, 0.988125000, 1.000000000]),
			("bounce.inOut", easingFunction::bounce::inOut as fn(f32) -> f32, [0.000000000, 0.030000000, 0.117187500, 0.500000000, 0.882812500, 0.970000000, 1.000000000]),
		]
	}

	//==============================================================================
	// 두 값이 거의 같은지 본다.
	//==============================================================================
	fn assertNear(actualValue: f32, expectedValue: f32) {
		assert!((actualValue - expectedValue).abs() < 1e-4, "{} != {}", actualValue, expectedValue);
	}

	//==============================================================================
	// 두 벡터가 거의 같은지 본다.
	//==============================================================================
	fn assertNearVector(actualVector: Vector2, expectedVector: Vector2) {
		assertNear(actualVector.x, expectedVector.x);
		assertNear(actualVector.y, expectedVector.y);
	}

	//==============================================================================
	// 불린 횟수를 세는 칸.
	//==============================================================================
	fn createCounter() -> Rc<Cell<u32>> {
		Rc::new(Cell::new(0))
	}

	//==============================================================================
	// 칸의 숫자를 하나 올린다.
	//==============================================================================
	fn increaseCounter(counter: &Rc<Cell<u32>>) {
		let previousCount = counter.get();
		counter.set(previousCount + 1);
	}

	//==============================================================================
	// 이징 값이 원본 공식과 같다. (표의 진행률 일곱 곳)
	//==============================================================================
	#[test]
	fn easingMatchesOriginalFormulas() {
		for (easingName, easing, expectedValues) in getEasingReferenceTable() {
			for (sampleIndex, progress) in EASING_SAMPLE_PROGRESSES.iter().enumerate() {
				let actualValue = easing(*progress) as f64;
				let expectedValue = expectedValues[sampleIndex];
				assert!((actualValue - expectedValue).abs() < 2e-5, "{} ({}): {} != {}", easingName, progress, actualValue, expectedValue);
			}
		}
	}

	//==============================================================================
	// 모든 이징은 0 에서 0, 1 에서 1 이다.
	//==============================================================================
	#[test]
	fn easingEndpointsAreZeroAndOne() {
		for (easingName, easing, _) in getEasingReferenceTable() {
			let startValue = easing(0.0);
			let endValue = easing(1.0);
			assert!(startValue.abs() < 1e-6, "{} (0): {}", easingName, startValue);
			assert!((endValue - 1.0).abs() < 1e-6, "{} (1): {}", easingName, endValue);
		}
	}

	//==============================================================================
	// start 전에는 돌지 않고, 대기 시간이 지나면 진행률대로 보간하고, 끝나면 완료 콜백을 한 번 부른다.
	//==============================================================================
	#[test]
	fn tweenWaitsDelayThenInterpolatesAndCompletesOnce() {
		let mut nodeTree = NodeTree::new();
		let updateLog: Rc<RefCell<Vec<f32>>> = Rc::new(RefCell::new(Vec::new()));
		let updateLogInCallback = updateLog.clone();
		let completeCounter = createCounter();
		let completeCounterInCallback = completeCounter.clone();
		let mut tween = Tween::new(TweenValues::new().with("value", 10.0))
			.to(TweenValues::new().with("value", 20.0), 1.0)
			.delay(0.5)
			.setUpdate(move |values, _nodeTree| {
				let value = values.getNumber("value").expect("value");
				updateLogInCallback.borrow_mut().push(value);
			})
			.setComplete(move |_nodeTree| {
				increaseCounter(&completeCounterInCallback);
			});

		// start 전.
		tween.tick(&mut nodeTree, 0.25);
		assert!(updateLog.borrow().is_empty());
		assert!(!tween.isFinished());

		// 대기 중.
		tween.start();
		tween.tick(&mut nodeTree, 0.25);
		assert!(updateLog.borrow().is_empty());

		// 대기가 끝난 순간은 진행률 0.
		tween.tick(&mut nodeTree, 0.25);
		tween.tick(&mut nodeTree, 0.5);
		assert_eq!(completeCounter.get(), 0);
		tween.tick(&mut nodeTree, 0.75);
		assert_eq!(*updateLog.borrow(), vec![10.0, 15.0, 20.0]);
		assert!(tween.isFinished());
		assert_eq!(completeCounter.get(), 1);

		// 끝난 뒤에는 아무것도 하지 않는다.
		tween.tick(&mut nodeTree, 1.0);
		assert_eq!(updateLog.borrow().len(), 3);
		assert_eq!(completeCounter.get(), 1);
	}

	//==============================================================================
	// 이징을 걸면 진행률 대신 이징 값으로 보간한다.
	//==============================================================================
	#[test]
	fn tweenAppliesEasing() {
		let mut nodeTree = NodeTree::new();
		let lastValue = Rc::new(Cell::new(0.0f32));
		let lastValueInCallback = lastValue.clone();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 100.0), 2.0)
			.easing(easingFunction::quadratic::r#in)
			.setUpdate(move |values, _nodeTree| {
				let value = values.getNumber("value").expect("value");
				lastValueInCallback.set(value);
			});
		tween.start();
		tween.tick(&mut nodeTree, 1.0);
		let easedValue = lastValue.get();
		assertNear(easedValue, 25.0);

		// 아무 함수나 이징으로 쓸 수 있다.
		let mut stepTween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 100.0), 2.0)
			.easing(|_progress| 0.0);
		stepTween.start();
		stepTween.tick(&mut nodeTree, 1.0);
		assert_eq!(stepTween.currentValues.getNumber("value"), Some(0.0));
	}

	//==============================================================================
	// 시작값에 없는 이름과 종류가 다른 이름은 건너뛰고, 목표값 순서로 담는다. Vector2 도 보간한다.
	//==============================================================================
	#[test]
	fn tweenSkipsMissingAndMismatchedValues() {
		let mut nodeTree = NodeTree::new();
		let initialValues = TweenValues::new().with("first", 1.0).with("vector", Vector2::zero()).with("mismatched", 5.0);
		let targetValues = TweenValues::new().with("vector", Vector2::create(2.0, 4.0)).with("missing", 1.0).with("mismatched", Vector2::one()).with("first", 3.0);
		let mut tween = Tween::new(initialValues).to(targetValues, 1.0);
		tween.start();
		tween.tick(&mut nodeTree, 0.5);

		let names: Vec<&str> = tween.currentValues.entries.iter().map(|entry| entry.name.as_ref()).collect();
		assert_eq!(names, vec!["vector", "first"]);
		let interpolatedVector = tween.currentValues.getVector2("vector").expect("vector");
		assertNearVector(interpolatedVector, Vector2::create(1.0, 2.0));
		let interpolatedNumber = tween.currentValues.getNumber("first").expect("first");
		assertNear(interpolatedNumber, 2.0);
		assert_eq!(tween.currentValues.getNumber("vector"), None);
	}

	//==============================================================================
	// 재생 중에 start 해도 처음부터 다시 하지 않는다. 끝난 뒤 start 하면 처음부터 다시 간다.
	//==============================================================================
	#[test]
	fn tweenStartWhilePlayingDoesNotRestart() {
		let mut nodeTree = NodeTree::new();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 1.0), 1.0);
		tween.start();
		tween.tick(&mut nodeTree, 0.5);
		tween.start();
		tween.tick(&mut nodeTree, 0.5);
		assert!(tween.isFinished());

		tween.start();
		assert!(!tween.isFinished());
		tween.tick(&mut nodeTree, 0.25);
		let restartedValue = tween.currentValues.getNumber("value").expect("value");
		assertNear(restartedValue, 0.25);

		// stop 은 완료 처리만 하고 완료 콜백은 부르지 않는다.
		tween.stop();
		assert!(tween.isFinished());
	}

	//==============================================================================
	// 길이가 0 이면 첫 tick 에 끝난다. (원본은 NaN 이 한 번 나올 수 있다)
	//==============================================================================
	#[test]
	fn zeroDurationFinishesOnFirstTick() {
		let mut nodeTree = NodeTree::new();
		let mut tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 8.0), 0.0);
		tween.start();
		tween.tick(&mut nodeTree, 0.0);
		assert!(tween.isFinished());
		assert_eq!(tween.currentValues.getNumber("value"), Some(8.0));
	}

	//==============================================================================
	// 노드 트윈은 tick 마다 NodeTree 를 거쳐 노드 속성에 쓴다.
	//==============================================================================
	#[test]
	fn nodeTweenWritesThroughNodeTree() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeId = nodeTree.createChild(rootId, "mover");
		let mut tween = Tween::createNodeTween(nodeId, TweenProperty::localPosition, Vector2::zero(), Vector2::create(100.0, 50.0), 1.0);
		assert_eq!(tween.getTargetNodeId(), Some(nodeId));
		tween.start();
		tween.tick(&mut nodeTree, 0.5);
		let halfwayPosition = nodeTree[nodeId].getLocalPosition();
		assertNearVector(halfwayPosition, Vector2::create(50.0, 25.0));
		tween.tick(&mut nodeTree, 0.5);
		let finalPosition = nodeTree[nodeId].getLocalPosition();
		assertNearVector(finalPosition, Vector2::create(100.0, 50.0));
		assert!(tween.isFinished());
	}

	//==============================================================================
	// Vector2 속성에 숫자를 주면 두 축에 같이 쓰고, 숫자 속성에 Vector2 를 주면 쓰지 않는다.
	//==============================================================================
	#[test]
	fn nodeTweenAppliesEachProperty() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeId = nodeTree.createChild(rootId, "target");
		let initialValues = TweenValues::new()
			.with(TweenProperty::localScale.getName(), 0.0)
			.with(TweenProperty::localRotation.getName(), 0.0)
			.with(TweenProperty::localOpacity.getName(), 1.0)
			.with(TweenProperty::contentSize.getName(), Vector2::create(10.0, 20.0))
			.with("custom", 0.0);
		let targetValues = TweenValues::new()
			.with("localScale", 2.0)
			.with("localRotation", 90.0)
			.with("localOpacity", 0.0)
			.with("contentSize", Vector2::create(30.0, 40.0))
			.with("custom", 4.0);
		let customValue = Rc::new(Cell::new(0.0f32));
		let customValueInCallback = customValue.clone();
		let mut tween = Tween::new(initialValues)
			.to(targetValues, 1.0)
			.setTarget(nodeId)
			.setUpdate(move |values, _nodeTree| {
				let custom = values.getNumber("custom").expect("custom");
				customValueInCallback.set(custom);
			});
		tween.start();
		tween.tick(&mut nodeTree, 0.5);
		let node = &nodeTree[nodeId];
		let localScale = node.getLocalScale();
		let localRotation = node.getLocalRotation();
		let localOpacity = node.getLocalOpacity();
		let contentSize = node.getContentSize();
		let custom = customValue.get();
		assertNearVector(localScale, Vector2::create(1.0, 1.0));
		assertNear(localRotation, 45.0);
		assertNear(localOpacity, 0.5);
		assertNearVector(contentSize, Vector2::create(20.0, 30.0));
		assertNear(custom, 2.0);

		let mut rotationNode = WorldNode::new("rotation");
		TweenProperty::localRotation.applyToNode(&mut rotationNode, TweenValue::vector2(Vector2::one()));
		assert_eq!(rotationNode.getLocalRotation(), 0.0);
		assert_eq!(TweenProperty::findByName("localOpacity"), Some(TweenProperty::localOpacity));
		assert_eq!(TweenProperty::findByName("custom"), None);
	}

	//==============================================================================
	// 대상 노드가 중간에 지워지면 콜백 없이 조용히 끝난다.
	//==============================================================================
	#[test]
	fn nodeTweenFinishesQuietlyWhenNodeDestroyed() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let nodeId = nodeTree.createChild(rootId, "doomed");
		let updateCounter = createCounter();
		let updateCounterInCallback = updateCounter.clone();
		let completeCounter = createCounter();
		let completeCounterInCallback = completeCounter.clone();
		let mut tween = Tween::createNodeTween(nodeId, TweenProperty::localPosition, Vector2::zero(), Vector2::create(10.0, 0.0), 1.0)
			.setUpdate(move |_values, _nodeTree| {
				increaseCounter(&updateCounterInCallback);
			})
			.setComplete(move |_nodeTree| {
				increaseCounter(&completeCounterInCallback);
			});
		tween.start();
		tween.tick(&mut nodeTree, 0.25);
		assert_eq!(updateCounter.get(), 1);

		nodeTree.destroyNode(nodeId);
		tween.tick(&mut nodeTree, 0.25);
		assert!(tween.isFinished());
		tween.tick(&mut nodeTree, 1.0);
		assert_eq!(updateCounter.get(), 1);
		assert_eq!(completeCounter.get(), 0);
	}

	//==============================================================================
	// 콜백은 노드 트리를 받아 다른 노드를 바꾸고 지울 수 있다.
	//==============================================================================
	#[test]
	fn tweenCallbacksCanEditNodeTree() {
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let fadingId = nodeTree.createChild(rootId, "fading");
		let removedId = nodeTree.createChild(rootId, "removed");
		let mut tween = Tween::new(TweenValues::new().with("alpha", 1.0))
			.to(TweenValues::new().with("alpha", 0.0), 1.0)
			.setUpdate(move |values, nodeTree| {
				let alpha = values.getNumber("alpha").expect("alpha");
				if let Some(fadingNode) = nodeTree.getNodeMut(fadingId) {
					fadingNode.setLocalOpacity(alpha);
				}
			})
			.setComplete(move |nodeTree| {
				nodeTree.destroyNode(removedId);
			});
		tween.start();
		tween.tick(&mut nodeTree, 0.5);
		let halfwayOpacity = nodeTree[fadingId].getLocalOpacity();
		assertNear(halfwayOpacity, 0.5);
		assert!(nodeTree.contains(removedId));
		tween.tick(&mut nodeTree, 0.5);
		let finalOpacity = nodeTree[fadingId].getLocalOpacity();
		assertNear(finalOpacity, 0.0);
		assert!(!nodeTree.contains(removedId));
	}

	//==============================================================================
	// 시험용 컴포넌트. (tick 마다 자기 노드 위치를 한 자리에 박는다)
	//==============================================================================
	struct PositionPinner {
		componentBase: ComponentBase,
		pinnedPosition: Vector2,
	}

	impl Component for PositionPinner {
		//==============================================================================
		// 공통 상태 반환.
		//==============================================================================
		fn getComponentBase(&self) -> &ComponentBase {
			&self.componentBase
		}

		//==============================================================================
		// 공통 상태 반환. (변경용)
		//==============================================================================
		fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
			&mut self.componentBase
		}

		//==============================================================================
		// 컴포넌트 이름.
		//==============================================================================
		fn getComponentType(&self) -> &'static str {
			"PositionPinner"
		}

		//==============================================================================
		// 주기적 갱신. (위치를 박는다)
		//==============================================================================
		fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
			let pinnedPosition = self.pinnedPosition;
			context.getTransformMut().setLocalPosition(pinnedPosition);
		}
	}

	//==============================================================================
	// 씬은 노드 tick 다음에 트윈을 돌리고(트윈이 이긴다), 끝난 트윈을 목록에서 뺀다.
	//==============================================================================
	#[test]
	fn sceneBaseTicksTweensAfterNodesAndRemovesFinished() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sceneBase = SceneBase::new();
		let rootId = sceneBase.getRootId();
		let nodeId = sceneBase.getNodeTreeMut().createChild(rootId, "pinned");
		sceneBase.getNodeTreeMut()[nodeId].addComponent(PositionPinner {
			componentBase: ComponentBase::new(),
			pinnedPosition: Vector2::create(999.0, 999.0),
		});
		let tween = Tween::createNodeTween(nodeId, TweenProperty::localPosition, Vector2::zero(), Vector2::create(10.0, 0.0), 1.0);
		let tweenId = sceneBase.startTween(tween);
		assert_eq!(sceneBase.getTweenCount(), 1);
		assert!(sceneBase.getTween(tweenId).is_some());

		sceneBase.tick(&engine, 0.5);
		let halfwayPosition = sceneBase.getNodeTree()[nodeId].getLocalPosition();
		assertNearVector(halfwayPosition, Vector2::create(5.0, 0.0));
		assert_eq!(sceneBase.getTweenCount(), 1);

		sceneBase.tick(&engine, 0.5);
		let finalPosition = sceneBase.getNodeTree()[nodeId].getLocalPosition();
		assertNearVector(finalPosition, Vector2::create(10.0, 0.0));
		assert_eq!(sceneBase.getTweenCount(), 0);
		assert!(sceneBase.getTween(tweenId).is_none());

		// 트윈이 빠진 뒤에는 컴포넌트 값이 남는다.
		sceneBase.tick(&engine, 0.5);
		let pinnedPosition = sceneBase.getNodeTree()[nodeId].getLocalPosition();
		assertNearVector(pinnedPosition, Vector2::create(999.0, 999.0));
	}

	//==============================================================================
	// stopTween 은 완료 콜백 없이 멈추고 빼며, 없는 핸들은 무시한다. stopAllTweens 는 모두 뺀다.
	//==============================================================================
	#[test]
	fn sceneBaseStopTweenSkipsComplete() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sceneBase = SceneBase::new();
		let completeCounter = createCounter();
		let completeCounterInCallback = completeCounter.clone();
		let tween = Tween::new(TweenValues::new().with("value", 0.0))
			.to(TweenValues::new().with("value", 1.0), 1.0)
			.setComplete(move |_nodeTree| {
				increaseCounter(&completeCounterInCallback);
			});
		let tweenId = sceneBase.startTween(tween);
		sceneBase.tick(&engine, 0.25);
		sceneBase.stopTween(tweenId);
		assert_eq!(sceneBase.getTweenCount(), 0);
		sceneBase.stopTween(tweenId);
		sceneBase.tick(&engine, 2.0);
		assert_eq!(completeCounter.get(), 0);

		for _ in 0..3 {
			let tween = Tween::new(TweenValues::new().with("value", 0.0)).to(TweenValues::new().with("value", 1.0), 1.0);
			sceneBase.startTween(tween);
		}
		assert_eq!(sceneBase.getTweenCount(), 3);
		sceneBase.stopAllTweens();
		assert_eq!(sceneBase.getTweenCount(), 0);
	}

	//==============================================================================
	// 같은 속성을 둘이 움직이면 먼저 시작한 트윈이 이긴다. (원본처럼 뒤에서부터 돈다)
	//==============================================================================
	#[test]
	fn sceneBaseEarlierTweenWinsOnSameProperty() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sceneBase = SceneBase::new();
		let rootId = sceneBase.getRootId();
		let nodeId = sceneBase.getNodeTreeMut().createChild(rootId, "contested");
		let firstTween = Tween::createNodeTween(nodeId, TweenProperty::localRotation, 100.0, 100.0, 1.0);
		let secondTween = Tween::createNodeTween(nodeId, TweenProperty::localRotation, 200.0, 200.0, 1.0);
		sceneBase.startTween(firstTween);
		sceneBase.startTween(secondTween);
		sceneBase.tick(&engine, 0.5);
		let contestedRotation = sceneBase.getNodeTree()[nodeId].getLocalRotation();
		assertNear(contestedRotation, 100.0);
	}

	//==============================================================================
	// 씬의 트윈은 노드가 지워지면 다음 tick 에 조용히 빠진다.
	//==============================================================================
	#[test]
	fn sceneBaseDropsTweenOfDestroyedNode() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut sceneBase = SceneBase::new();
		let rootId = sceneBase.getRootId();
		let nodeId = sceneBase.getNodeTreeMut().createChild(rootId, "doomed");
		let tween = Tween::createNodeTween(nodeId, TweenProperty::localOpacity, 1.0, 0.0, 1.0);
		let tweenId = sceneBase.startTween(tween);
		sceneBase.tick(&engine, 0.25);
		sceneBase.getNodeTreeMut().destroyNode(nodeId);
		sceneBase.tick(&engine, 0.25);
		assert_eq!(sceneBase.getTweenCount(), 0);
		assert!(sceneBase.getTween(tweenId).is_none());
	}

	//==============================================================================
	// 값 묶음: 같은 이름은 바꾸고, 없는 이름이나 다른 종류는 None.
	//==============================================================================
	#[test]
	fn tweenValuesSetReplacesSameName() {
		let mut values = TweenValues::new().with("x", 1.0);
		values.set("x", 2.0);
		values.set(String::from("y"), Vector2::create(3.0, 4.0));
		assert_eq!(values.getCount(), 2);
		assert_eq!(values.getNumber("x"), Some(2.0));
		assert_eq!(values.getVector2("y"), Some(Vector2::create(3.0, 4.0)));
		assert_eq!(values.getNumber("y"), None);
		assert_eq!(values.get("z"), None);
	}
}
