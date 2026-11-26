//==============================================================================
//! 애니메이터. (상태 / 전환 / 프로퍼티를 드는 애니메이션 상태 기계의 뼈대)
//! - 원본 animation/animator.js 를 옮긴 것이다. 원본은 아직 뼈대뿐이다:
//!   Animator 는 Animation 처리기에 tick 을 넘기고 프로퍼티 사전(이름 → 값)을 든다.
//!   AnimationState(클립 하나)와 AnimationTransition(from / to)은 필드만 있고 쓰는 곳이 없으며, 전환을 고르는 논리도 없다.
//!   러스트 판도 그 범위만 옮기고 전환 논리를 지어내지 않았다.
//! - 읽고 쓰는 함수(AnimationState.set_clip / get_clip, AnimationTransition.set_from / get_from / set_to / get_to,
//!   Animator.get_animation / add_state / get_states / add_transition / get_transitions)는 원본과 같다.
//!   전환의 from / to 는 원본처럼 Animator 상태 목록의 번호다.
//! - 원본과 다른 점:
//!   - 프로퍼티 값은 원본 아무 값 대신 serde_json::Value 다. (숫자, 참거짓, 글 모두 담는다. 러스트는 값의 타입을 정해야 한다)
//!   - get_property 는 없으면 None 이다. (원본 undefined)
//!   - 원본 get_states / get_transitions 는 List 를, 러스트 판은 조각(&`[..]`)을 돌려준다. get_animation_mut 은 빌림 규칙 때문에 따로 있다.
//==============================================================================
use crate::core::animation::Animation;
use crate::resource::animationclip::AnimationClip;
use serde_json::Value;
use std::collections::HashMap;

//==============================================================================
/// 애니메이션 상태. (클립 하나)
/// - 원본처럼 클립을 들기만 한다. Animator 가 이것을 재생하지는 않는다.
///
/// # 예제
/// 반복 클립을 든 상태
/// ```rust
/// use vanilla::AnimationState;
/// use vanilla::resource::animationclip::AnimationClip;
///
/// let mut clip = AnimationClip::new();
/// clip.set_loop(true);
/// let mut state = AnimationState::new();
/// assert!(state.get_clip().is_none());
/// state.set_clip(Some(clip));
/// assert!(state.get_clip().is_some_and(|clip| clip.is_loop()));
/// ```
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimationState {
	clip: Option<AnimationClip>,
}

impl AnimationState {
	//==============================================================================
	/// 생성. (클립 없음)
	///
	/// # 돌려줌
	/// 클립이 없는 상태.
	//==============================================================================
	pub fn new() -> AnimationState {
		AnimationState {
			clip: None,
		}
	}

	//==============================================================================
	/// 클립 설정.
	///
	/// # 인자
	/// - `clip`: 이 상태의 클립. None 이면 클립을 비운다.
	//==============================================================================
	pub fn set_clip(&mut self, clip: Option<AnimationClip>) {
		self.clip = clip;
	}

	//==============================================================================
	/// 클립 반환.
	///
	/// # 돌려줌
	/// 든 클립의 참조. 클립이 없으면 None.
	//==============================================================================
	pub fn get_clip(&self) -> Option<&AnimationClip> {
		self.clip.as_ref()
	}
}

//==============================================================================
/// 애니메이션 전환. (조건이 만족되면 대상 상태를 실행하는 상태 간 연결. 상태는 Animator 상태 목록 번호)
/// - 원본처럼 출발과 도착 번호만 든다. 조건을 묻거나 전환을 고르는 논리는 없다.
///
/// # 예제
/// 0 번 상태에서 1 번 상태로 가는 전환
/// ```rust
/// use vanilla::AnimationTransition;
///
/// let mut transition = AnimationTransition::new();
/// assert_eq!(transition.get_from(), None);
/// transition.set_from(Some(0));
/// transition.set_to(Some(1));
/// assert_eq!(transition.get_from(), Some(0));
/// assert_eq!(transition.get_to(), Some(1));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AnimationTransition {
	from: Option<usize>,
	to: Option<usize>,
}

impl AnimationTransition {
	//==============================================================================
	/// 생성. (양쪽 비어 있음)
	///
	/// # 돌려줌
	/// 출발과 도착이 모두 None 인 전환.
	//==============================================================================
	pub fn new() -> AnimationTransition {
		AnimationTransition {
			from: None,
			to: None,
		}
	}

	//==============================================================================
	/// 출발 상태 설정.
	///
	/// # 인자
	/// - `from`: 출발 상태의 Animator 상태 목록 번호. (add_state 가 돌려준 번호) None 이면 비운다.
	//==============================================================================
	pub fn set_from(&mut self, from: Option<usize>) {
		self.from = from;
	}

	//==============================================================================
	/// 출발 상태 반환.
	///
	/// # 돌려줌
	/// 출발 상태의 목록 번호. 정하지 않았으면 None.
	//==============================================================================
	pub fn get_from(&self) -> Option<usize> {
		self.from
	}

	//==============================================================================
	/// 도착 상태 설정.
	///
	/// # 인자
	/// - `to`: 도착 상태의 Animator 상태 목록 번호. (add_state 가 돌려준 번호) None 이면 비운다.
	//==============================================================================
	pub fn set_to(&mut self, to: Option<usize>) {
		self.to = to;
	}

	//==============================================================================
	/// 도착 상태 반환.
	///
	/// # 돌려줌
	/// 도착 상태의 목록 번호. 정하지 않았으면 None.
	//==============================================================================
	pub fn get_to(&self) -> Option<usize> {
		self.to
	}
}

//==============================================================================
/// 애니메이터.
/// - Animation 처리기 하나와 상태 목록, 전환 목록, 프로퍼티 사전(이름 → 값)을 든다. tick 은 처리기에만 넘긴다.
///
/// # 예제
/// 상태 둘과 전환 하나, 프로퍼티, 프레임 넘기기
/// ```rust
/// use vanilla::{AnimationState, AnimationTransition, Animator, Image, Rect};
///
/// let mut animator = Animator::new();
/// let idle_index = animator.add_state(AnimationState::new());
/// let run_index = animator.add_state(AnimationState::new());
/// let mut transition = AnimationTransition::new();
/// transition.set_from(Some(idle_index));
/// transition.set_to(Some(run_index));
/// animator.add_transition(transition);
/// animator.set_property("speed", 1.5);
/// assert_eq!(animator.get_property("speed").and_then(|value| value.as_f64()), Some(1.5));
/// assert_eq!(animator.get_transitions()[0].get_to(), Some(run_index));
///
/// let image = Image::create_from_pixels(32, 16, vec![0u8; 32 * 16 * 4]);
/// animator.get_animation_mut().set_frames_from_rects(image, &[Rect::create(0.0, 0.0, 16.0, 16.0), Rect::create(16.0, 0.0, 16.0, 16.0)]);
/// animator.get_animation_mut().set_animation_speed(10.0);
/// animator.get_animation_mut().play();
/// animator.tick(0.15);
/// assert_eq!(animator.get_animation().get_current_frame_index(), 1);
/// ```
//==============================================================================
pub struct Animator {
	animation: Animation,
	states: Vec<AnimationState>,
	transitions: Vec<AnimationTransition>,
	properties: HashMap<String, Value>,
}

impl Animator {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 처리기와 빈 상태, 전환, 프로퍼티를 든 애니메이터.
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
	/// 갱신. (처리기에 넘긴다)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임에서 흐른 시간. (초)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		self.animation.tick(time_delta);
	}

	//==============================================================================
	/// 프로퍼티 설정. (같은 이름이 있으면 바꾼다)
	///
	/// # 인자
	/// - `name`: 프로퍼티 이름.
	/// - `value`: 값. (숫자, 참거짓, 글처럼 `serde_json::Value` 로 바뀌는 것)
	///
	/// # 예제
	/// 숫자, 참거짓, 글 프로퍼티
	/// ```rust
	/// use vanilla::Animator;
	///
	/// let mut animator = Animator::new();
	/// animator.set_property("speed", 1.5);
	/// animator.set_property("is_grounded", true);
	/// animator.set_property("mode", "run");
	/// animator.set_property("speed", 3);
	/// assert_eq!(animator.get_property("speed").and_then(|value| value.as_i64()), Some(3));
	/// assert_eq!(animator.get_property("is_grounded").and_then(|value| value.as_bool()), Some(true));
	/// assert_eq!(animator.get_property("mode").and_then(|value| value.as_str()), Some("run"));
	/// assert!(animator.get_property("missing").is_none());
	/// assert_eq!(animator.get_properties().len(), 3);
	/// ```
	//==============================================================================
	pub fn set_property(&mut self, name: &str, value: impl Into<Value>) {
		self.properties.insert(name.to_string(), value.into());
	}

	//==============================================================================
	/// 프로퍼티 반환. (없으면 None)
	///
	/// # 인자
	/// - `name`: 프로퍼티 이름.
	///
	/// # 돌려줌
	/// 그 이름의 값. 없으면 None.
	//==============================================================================
	pub fn get_property(&self, name: &str) -> Option<&Value> {
		self.properties.get(name)
	}

	//==============================================================================
	/// 모든 프로퍼티 반환.
	///
	/// # 돌려줌
	/// 이름 → 값 사전.
	//==============================================================================
	pub fn get_properties(&self) -> &HashMap<String, Value> {
		&self.properties
	}

	//==============================================================================
	/// 처리기 반환.
	///
	/// # 돌려줌
	/// 프레임 애니메이션 처리기의 참조.
	//==============================================================================
	pub fn get_animation(&self) -> &Animation {
		&self.animation
	}

	//==============================================================================
	/// 처리기 반환. (변경용, 프레임을 넣고 재생할 때)
	///
	/// # 돌려줌
	/// 프레임 애니메이션 처리기의 변경 참조.
	//==============================================================================
	pub fn get_animation_mut(&mut self) -> &mut Animation {
		&mut self.animation
	}

	//==============================================================================
	/// 상태 더하기. (번호를 돌려준다)
	///
	/// # 인자
	/// - `state`: 목록 끝에 넣을 상태.
	///
	/// # 돌려줌
	/// 넣은 상태의 목록 번호. (0 부터, 전환의 from 과 to 에 쓴다)
	//==============================================================================
	pub fn add_state(&mut self, state: AnimationState) -> usize {
		self.states.push(state);
		self.states.len() - 1
	}

	//==============================================================================
	/// 상태 목록 반환.
	///
	/// # 돌려줌
	/// 넣은 차례대로의 상태 조각.
	//==============================================================================
	pub fn get_states(&self) -> &[AnimationState] {
		&self.states
	}

	//==============================================================================
	/// 전환 더하기. (번호를 돌려준다)
	///
	/// # 인자
	/// - `transition`: 목록 끝에 넣을 전환. (from 과 to 가 상태 목록에 있는지는 보지 않는다)
	///
	/// # 돌려줌
	/// 넣은 전환의 목록 번호. (0 부터)
	//==============================================================================
	pub fn add_transition(&mut self, transition: AnimationTransition) -> usize {
		self.transitions.push(transition);
		self.transitions.len() - 1
	}

	//==============================================================================
	/// 전환 목록 반환.
	///
	/// # 돌려줌
	/// 넣은 차례대로의 전환 조각.
	//==============================================================================
	pub fn get_transitions(&self) -> &[AnimationTransition] {
		&self.transitions
	}
}

impl Default for Animator {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
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
	/// 프로퍼티는 이름으로 넣고 바꾸고 꺼낸다. (없으면 None)
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
	/// tick 은 처리기에 넘긴다. (재생 중인 Animation 의 프레임이 넘어간다)
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
	/// 상태와 전환은 목록 번호로 잇는다.
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
