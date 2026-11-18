//==============================================================================
// 애니메이터. (상태 / 전환 / 프로퍼티를 드는 애니메이션 상태 기계의 뼈대)
// - 원본 experimental/animation/animator.js 를 옮긴 것이다. 원본은 아직 뼈대뿐이다:
//   Animator 는 Animation 처리기에 tick 을 넘기고 프로퍼티 사전(이름 → 값)을 든다.
//   AnimationState(클립 하나)와 AnimationTransition(from / to)은 필드만 있고 쓰는 곳이 없으며, 전환을 고르는 논리도 없다.
//   러스트 판도 그 범위만 옮기고 전환 논리를 지어내지 않았다.
// - 원본과 다른 점:
//   - 원본 private 필드는 읽는 곳이 없어 러스트에서는 쓰이지 않는 필드가 된다. 그래서 읽고 쓰는 함수를 더했다:
//     AnimationState.setClip / getClip, AnimationTransition.setFrom / getFrom / setTo / getTo,
//     Animator.getAnimation(Mut) / addState / getStates / addTransition / getTransitions.
//   - 전환의 from / to 는 상태 객체 참조 대신 Animator 상태 목록의 번호다.
//   - 프로퍼티 값은 원본 아무 값 대신 serde_json::Value 다. (숫자, 참거짓, 글 모두 담는다)
//   - getProperty 는 없으면 None 이다. (원본 undefined)
//==============================================================================
use crate::core::animation::Animation;
use crate::resource::animationclip::AnimationClip;
use serde_json::Value;
use std::collections::HashMap;

//==============================================================================
// 애니메이션 상태. (클립 하나)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationState {
	clip: Option<AnimationClip>,
}

impl AnimationState {
	//==============================================================================
	// 생성. (클립 없음)
	//==============================================================================
	pub fn new() -> AnimationState {
		AnimationState {
			clip: None,
		}
	}

	//==============================================================================
	// 클립 설정.
	//==============================================================================
	pub fn setClip(&mut self, clip: Option<AnimationClip>) {
		self.clip = clip;
	}

	//==============================================================================
	// 클립 반환.
	//==============================================================================
	pub fn getClip(&self) -> Option<&AnimationClip> {
		self.clip.as_ref()
	}
}

//==============================================================================
// 애니메이션 전환. (조건이 만족되면 대상 상태를 실행하는 상태 간 연결. 상태는 Animator 상태 목록 번호)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationTransition {
	from: Option<usize>,
	to: Option<usize>,
}

impl AnimationTransition {
	//==============================================================================
	// 생성. (양쪽 비어 있음)
	//==============================================================================
	pub fn new() -> AnimationTransition {
		AnimationTransition {
			from: None,
			to: None,
		}
	}

	//==============================================================================
	// 출발 상태 설정.
	//==============================================================================
	pub fn setFrom(&mut self, from: Option<usize>) {
		self.from = from;
	}

	//==============================================================================
	// 출발 상태 반환.
	//==============================================================================
	pub fn getFrom(&self) -> Option<usize> {
		self.from
	}

	//==============================================================================
	// 도착 상태 설정.
	//==============================================================================
	pub fn setTo(&mut self, to: Option<usize>) {
		self.to = to;
	}

	//==============================================================================
	// 도착 상태 반환.
	//==============================================================================
	pub fn getTo(&self) -> Option<usize> {
		self.to
	}
}

//==============================================================================
// 애니메이터.
//==============================================================================
pub struct Animator {
	animation: Animation,
	states: Vec<AnimationState>,
	transitions: Vec<AnimationTransition>,
	properties: HashMap<String, Value>,
}

impl Animator {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Animator {
		Animator {
			animation: Animation::new(),
			states: Vec::new(),
			transitions: Vec::new(),
			properties: HashMap::new(),
		}
	}

	//==============================================================================
	// 갱신. (처리기에 넘긴다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		self.animation.tick(timeDelta);
	}

	//==============================================================================
	// 프로퍼티 설정. (같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn setProperty(&mut self, name: &str, value: impl Into<Value>) {
		self.properties.insert(name.to_string(), value.into());
	}

	//==============================================================================
	// 프로퍼티 반환. (없으면 None)
	//==============================================================================
	pub fn getProperty(&self, name: &str) -> Option<&Value> {
		self.properties.get(name)
	}

	//==============================================================================
	// 모든 프로퍼티 반환.
	//==============================================================================
	pub fn getProperties(&self) -> &HashMap<String, Value> {
		&self.properties
	}

	//==============================================================================
	// 처리기 반환.
	//==============================================================================
	pub fn getAnimation(&self) -> &Animation {
		&self.animation
	}

	//==============================================================================
	// 처리기 반환. (변경용, 프레임을 넣고 재생할 때)
	//==============================================================================
	pub fn getAnimationMut(&mut self) -> &mut Animation {
		&mut self.animation
	}

	//==============================================================================
	// 상태 더하기. (번호를 돌려준다)
	//==============================================================================
	pub fn addState(&mut self, state: AnimationState) -> usize {
		self.states.push(state);
		self.states.len() - 1
	}

	//==============================================================================
	// 상태 목록 반환.
	//==============================================================================
	pub fn getStates(&self) -> &[AnimationState] {
		&self.states
	}

	//==============================================================================
	// 전환 더하기. (번호를 돌려준다)
	//==============================================================================
	pub fn addTransition(&mut self, transition: AnimationTransition) -> usize {
		self.transitions.push(transition);
		self.transitions.len() - 1
	}

	//==============================================================================
	// 전환 목록 반환.
	//==============================================================================
	pub fn getTransitions(&self) -> &[AnimationTransition] {
		&self.transitions
	}
}

impl Default for Animator {
	fn default() -> Animator {
		Animator::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::rect::Rect;
	use crate::core::image::Image;
	use serde_json::json;

	//==============================================================================
	// 프로퍼티는 이름으로 넣고 바꾸고 꺼낸다. (없으면 None)
	//==============================================================================
	#[test]
	fn propertiesSetReplaceAndGet() {
		let mut animator = Animator::new();
		animator.setProperty("speed", 1.5);
		animator.setProperty("isGrounded", true);
		animator.setProperty("speed", 3);
		assert_eq!(animator.getProperty("speed"), Some(&json!(3)));
		assert_eq!(animator.getProperty("isGrounded"), Some(&json!(true)));
		assert_eq!(animator.getProperty("missing"), None);
		assert_eq!(animator.getProperties().len(), 2);
	}

	//==============================================================================
	// tick 은 처리기에 넘긴다. (재생 중인 Animation 의 프레임이 넘어간다)
	//==============================================================================
	#[test]
	fn tickAdvancesAnimation() {
		let image = Image::createFromPixels(32, 16, vec![0u8; 32 * 16 * 4]);
		let mut animator = Animator::new();
		animator.getAnimationMut().setFramesFromRects(image, &[Rect::create(0.0, 0.0, 16.0, 16.0), Rect::create(16.0, 0.0, 16.0, 16.0)]);
		animator.getAnimationMut().setAnimationSpeed(10.0);
		animator.getAnimationMut().play();
		animator.tick(0.15);
		assert_eq!(animator.getAnimation().getCurrentFrameIndex(), 1);
	}

	//==============================================================================
	// 상태와 전환은 목록 번호로 잇는다.
	//==============================================================================
	#[test]
	fn statesAndTransitionsAreLinkedByIndex() {
		let mut animator = Animator::new();
		let mut idleState = AnimationState::new();
		let mut idleClip = AnimationClip::new();
		idleClip.setLoop(true);
		idleState.setClip(Some(idleClip));
		let idleIndex = animator.addState(idleState);
		let runIndex = animator.addState(AnimationState::new());
		let mut transition = AnimationTransition::new();
		transition.setFrom(Some(idleIndex));
		transition.setTo(Some(runIndex));
		animator.addTransition(transition);
		assert_eq!(animator.getStates().len(), 2);
		assert!(animator.getStates()[idleIndex].getClip().is_some_and(|clip| clip.isLoop()));
		assert!(animator.getStates()[runIndex].getClip().is_none());
		assert_eq!(animator.getTransitions()[0].getFrom(), Some(0));
		assert_eq!(animator.getTransitions()[0].getTo(), Some(1));
	}
}
