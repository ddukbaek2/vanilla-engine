//==============================================================================
// 애니메이터. (상태 / 전환 / 프로퍼티를 드는 애니메이션 상태 기계의 뼈대)
// - 원본 experimental/animation/animator.js 를 옮긴 것이다. 원본은 아직 뼈대뿐이다:
//   Animator 는 Animation 처리기에 tick 을 넘기고 프로퍼티 사전(이름 → 값)을 든다.
//   AnimationState(클립 하나)와 AnimationTransition(from / to)은 필드만 있고 쓰는 곳이 없으며, 전환을 고르는 논리도 없다.
//   러스트 판도 그 범위만 옮기고 전환 논리를 지어내지 않았다.
// - 원본과 다른 점:
//   - 원본 private 필드는 읽는 곳이 없어 러스트에서는 쓰이지 않는 필드가 된다. 그래서 읽고 쓰는 함수를 더했다:
//     AnimationState.set_clip / get_clip, AnimationTransition.set_from / get_from / set_to / get_to,
//     Animator.get_animation(Mut) / add_state / get_states / add_transition / get_transitions.
//   - 전환의 from / to 는 상태 객체 참조 대신 Animator 상태 목록의 번호다.
//   - 프로퍼티 값은 원본 아무 값 대신 serde_json::Value 다. (숫자, 참거짓, 글 모두 담는다)
//   - get_property 는 없으면 None 이다. (원본 undefined)
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
	pub fn set_clip(&mut self, clip: Option<AnimationClip>) {
		self.clip = clip;
	}

	//==============================================================================
	// 클립 반환.
	//==============================================================================
	pub fn get_clip(&self) -> Option<&AnimationClip> {
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
	pub fn set_from(&mut self, from: Option<usize>) {
		self.from = from;
	}

	//==============================================================================
	// 출발 상태 반환.
	//==============================================================================
	pub fn get_from(&self) -> Option<usize> {
		self.from
	}

	//==============================================================================
	// 도착 상태 설정.
	//==============================================================================
	pub fn set_to(&mut self, to: Option<usize>) {
		self.to = to;
	}

	//==============================================================================
	// 도착 상태 반환.
	//==============================================================================
	pub fn get_to(&self) -> Option<usize> {
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
	pub fn tick(&mut self, time_delta: f32) {
		self.animation.tick(time_delta);
	}

	//==============================================================================
	// 프로퍼티 설정. (같은 이름이 있으면 바꾼다)
	//==============================================================================
	pub fn set_property(&mut self, name: &str, value: impl Into<Value>) {
		self.properties.insert(name.to_string(), value.into());
	}

	//==============================================================================
	// 프로퍼티 반환. (없으면 None)
	//==============================================================================
	pub fn get_property(&self, name: &str) -> Option<&Value> {
		self.properties.get(name)
	}

	//==============================================================================
	// 모든 프로퍼티 반환.
	//==============================================================================
	pub fn get_properties(&self) -> &HashMap<String, Value> {
		&self.properties
	}

	//==============================================================================
	// 처리기 반환.
	//==============================================================================
	pub fn get_animation(&self) -> &Animation {
		&self.animation
	}

	//==============================================================================
	// 처리기 반환. (변경용, 프레임을 넣고 재생할 때)
	//==============================================================================
	pub fn get_animation_mut(&mut self) -> &mut Animation {
		&mut self.animation
	}

	//==============================================================================
	// 상태 더하기. (번호를 돌려준다)
	//==============================================================================
	pub fn add_state(&mut self, state: AnimationState) -> usize {
		self.states.push(state);
		self.states.len() - 1
	}

	//==============================================================================
	// 상태 목록 반환.
	//==============================================================================
	pub fn get_states(&self) -> &[AnimationState] {
		&self.states
	}

	//==============================================================================
	// 전환 더하기. (번호를 돌려준다)
	//==============================================================================
	pub fn add_transition(&mut self, transition: AnimationTransition) -> usize {
		self.transitions.push(transition);
		self.transitions.len() - 1
	}

	//==============================================================================
	// 전환 목록 반환.
	//==============================================================================
	pub fn get_transitions(&self) -> &[AnimationTransition] {
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
	fn properties_set_replace_and_get() {
		let mut animator = Animator::new();
		animator.set_property("speed", 1.5);
		animator.set_property("isGrounded", true);
		animator.set_property("speed", 3);
		assert_eq!(animator.get_property("speed"), Some(&json!(3)));
		assert_eq!(animator.get_property("isGrounded"), Some(&json!(true)));
		assert_eq!(animator.get_property("missing"), None);
		assert_eq!(animator.get_properties().len(), 2);
	}

	//==============================================================================
	// tick 은 처리기에 넘긴다. (재생 중인 Animation 의 프레임이 넘어간다)
	//==============================================================================
	#[test]
	fn tick_advances_animation() {
		let image = Image::create_from_pixels(32, 16, vec![0u8; 32 * 16 * 4]);
		let mut animator = Animator::new();
		animator.get_animation_mut().set_frames_from_rects(image, &[Rect::create(0.0, 0.0, 16.0, 16.0), Rect::create(16.0, 0.0, 16.0, 16.0)]);
		animator.get_animation_mut().set_animation_speed(10.0);
		animator.get_animation_mut().play();
		animator.tick(0.15);
		assert_eq!(animator.get_animation().get_current_frame_index(), 1);
	}

	//==============================================================================
	// 상태와 전환은 목록 번호로 잇는다.
	//==============================================================================
	#[test]
	fn states_and_transitions_are_linked_by_index() {
		let mut animator = Animator::new();
		let mut idle_state = AnimationState::new();
		let mut idle_clip = AnimationClip::new();
		idle_clip.set_loop(true);
		idle_state.set_clip(Some(idle_clip));
		let idle_index = animator.add_state(idle_state);
		let run_index = animator.add_state(AnimationState::new());
		let mut transition = AnimationTransition::new();
		transition.set_from(Some(idle_index));
		transition.set_to(Some(run_index));
		animator.add_transition(transition);
		assert_eq!(animator.get_states().len(), 2);
		assert!(animator.get_states()[idle_index].get_clip().is_some_and(|clip| clip.is_loop()));
		assert!(animator.get_states()[run_index].get_clip().is_none());
		assert_eq!(animator.get_transitions()[0].get_from(), Some(0));
		assert_eq!(animator.get_transitions()[0].get_to(), Some(1));
	}
}
