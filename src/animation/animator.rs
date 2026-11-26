//==============================================================================
//! 애니메이터. (상태 기계로 스프라이트 클립을 고르고 섞는다)
//! - 원본 experimental/animation/animator.js 를 옮긴 것이다. 상태(클립, 속도, 되풀이), 매개변수(실수, 정수, 참 거짓, 트리거),
//!   전이(조건 목록, 종료 시각, 섞는 시간, 어느 상태에서나 넘어가는 전이), 매 프레임 tick 의 전이 판정과 크로스페이드 가중치가 원본과 같다.
//! - 처음 더한 상태에서 시작한다. play 로 아무 상태로나 바로 옮길 수 있다. 전이의 from, to 는 Animator 상태 목록의 번호이고 from 이 None 이면 어느 상태에서나(Any State) 넘어간다.
//! - tick 차례: 상태 시간을 흘린다(섞는 중이면 두 상태 모두) → 섞는 중이면 섞는 시간을 다 채웠을 때 다음 상태로 넘어간다
//!   → 섞는 중이 아니면 어느 상태에서나 넘어가는 전이, 지금 상태에서 나가는 전이 차례로 넣은 순서대로 보고 처음 맞는 하나를 탄다.
//!   섞는 동안에는 새 전이를 보지 않는다. 조건으로 쓴 트리거는 그 전이가 타면 풀린다.
//! - 종료 시각은 나가는 상태의 정규화 시간이다. 그 시각을 지나는 갱신에서만 넘어갈 수 있고, 되풀이하는 상태에서 1 보다 작은 종료 시각은 바퀴마다 본다.
//! - 원본과 다른 점:
//!   - 원본 get_states / get_transitions 는 List 를, 러스트 판은 조각(&`[..]`)을 돌려준다. 원본 getParameters 의 사전은 러스트 판에 없다. (종류별 게터로 읽는다)
//!   - 원본 조건은 { parameterName, mode, threshold } 객체이고 러스트 판은 같은 필드의 AnimationCondition 이다. 원본 AnimationConditionMode 의 글 값은 열거 값이다.
//!   - 정수 매개변수는 i32, 실수와 기준 값은 f32 다. (원본은 모두 수) 정수를 기준 값과 견줄 때는 f32 로 바꿔 견준다.
//==============================================================================
use crate::base::rect::Rect;
use crate::resource::animationclip::AnimationClip;
use std::collections::HashMap;

//==============================================================================
/// 전이 조건의 견주는 법.
/// - If, IfNot 은 참 거짓 매개변수를 본다. (If 는 트리거가 걸려 있는 것도 본다)
/// - Greater, Less, Equals, NotEqual 은 실수와 정수 매개변수를 기준 값과 견준다.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationConditionMode {
	/// 참 거짓 매개변수가 참이거나 트리거가 걸려 있다.
	If,
	/// 참 거짓 매개변수가 거짓이다.
	IfNot,
	/// 수 매개변수가 기준 값보다 크다.
	Greater,
	/// 수 매개변수가 기준 값보다 작다.
	Less,
	/// 수 매개변수가 기준 값과 같다.
	Equals,
	/// 수 매개변수가 기준 값과 다르다.
	NotEqual,
}

//==============================================================================
/// 전이 조건. (원본 { parameterName, mode, threshold })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationCondition {
	/// 견줄 매개변수 이름.
	pub parameter_name: String,
	/// 견주는 법.
	pub mode: AnimationConditionMode,
	/// 기준 값. (Greater, Less, Equals, NotEqual 에서 쓴다)
	pub threshold: f32,
}

//==============================================================================
/// 애니메이터 매개변수. (원본 { type, value }, 트리거는 걸려 있는지)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
enum AnimatorParameter {
	Float(f32),
	Int(i32),
	Bool(bool),
	Trigger(bool),
}

//==============================================================================
/// 애니메이션 상태. (클립 하나를 속도와 되풀이 여부로 튼다)
/// - 상태 시간은 들어온 뒤 흐른 초에 속도를 곱해 쌓은 것이다. 정규화 시간은 상태 시간을 클립 길이로 나눈 것이다. (1 이면 한 바퀴)
/// - 되풀이는 set_loop 로 정하지 않으면(None) 클립의 is_loop 를 따른다.
/// - 클립이 없거나 길이가 0 이면 정규화 시간은 시간이 흐르기 전 0, 흐른 뒤 1 이다.
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationState {
	clip: Option<AnimationClip>,
	speed: f32,
	is_loop: Option<bool>,
}

impl AnimationState {
	//==============================================================================
	/// 생성. (클립 없음, 속도 1, 되풀이는 클립을 따름)
	//==============================================================================
	pub fn new() -> AnimationState {
		AnimationState {
			clip: None,
			speed: 1.0,
			is_loop: None,
		}
	}

	//==============================================================================
	/// 클립 설정.
	///
	/// # 인자
	/// - `clip`: 이 상태에서 쓸 클립. (None 이면 비운다)
	//==============================================================================
	pub fn set_clip(&mut self, clip: Option<AnimationClip>) {
		self.clip = clip;
	}

	//==============================================================================
	/// 클립 반환.
	///
	/// # 돌려줌
	/// 이 상태의 클립. 없으면 None.
	//==============================================================================
	pub fn get_clip(&self) -> Option<&AnimationClip> {
		self.clip.as_ref()
	}

	//==============================================================================
	/// 재생 속도 설정.
	///
	/// # 인자
	/// - `speed`: 재생 속도. (기본 1, 2 면 두 배로 빠르고 0 이면 멈춘다)
	//==============================================================================
	pub fn set_speed(&mut self, speed: f32) {
		self.speed = speed;
	}

	//==============================================================================
	/// 재생 속도 반환.
	///
	/// # 돌려줌
	/// 재생 속도. (기본 1)
	//==============================================================================
	pub fn get_speed(&self) -> f32 {
		self.speed
	}

	//==============================================================================
	/// 되풀이 여부 설정.
	///
	/// # 인자
	/// - `is_loop`: 되풀이하면 참. (None 이면 클립의 is_loop 를 따른다, 기본 None)
	//==============================================================================
	pub fn set_loop(&mut self, is_loop: Option<bool>) {
		self.is_loop = is_loop;
	}

	//==============================================================================
	/// 되풀이 여부 반환.
	///
	/// # 돌려줌
	/// 되풀이하면 참. (set_loop 로 정하지 않았으면 클립의 is_loop, 클립도 없으면 거짓)
	//==============================================================================
	pub fn is_loop(&self) -> bool {
		match self.is_loop {
			Some(is_loop) => {
				is_loop
			}
			None => {
				self.clip.as_ref().is_some_and(|clip| clip.is_loop())
			}
		}
	}

	//==============================================================================
	/// 한 바퀴 길이 반환.
	///
	/// # 돌려줌
	/// 클립의 지속 시간. (초, 클립이 없으면 0)
	//==============================================================================
	pub fn get_duration(&self) -> f32 {
		match self.clip.as_ref() {
			Some(clip) => {
				clip.get_duration()
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 정규화 시간 반환.
	///
	/// # 인자
	/// - `time`: 상태 시간. (초, 속도를 곱해 쌓은 것)
	///
	/// # 돌려줌
	/// 상태 시간을 한 바퀴 길이로 나눈 것. (1 이면 한 바퀴, 되풀이하면 1 을 넘어 쌓인다. 길이가 0 이면 시간이 흐르기 전 0, 흐른 뒤 1)
	//==============================================================================
	pub fn get_normalized_time(&self, time: f32) -> f32 {
		let duration = self.get_duration();
		if duration <= 0.0 {
			return if time > 0.0 {
				1.0
			}
			else {
				0.0
			};
		}
		time / duration
	}

	//==============================================================================
	/// 프레임 번호 반환.
	///
	/// # 인자
	/// - `time`: 상태 시간. (초, 속도를 곱해 쌓은 것)
	///
	/// # 돌려줌
	/// 그 시간에 보일 클립 프레임의 번호. (0 부터, 되풀이하면 바퀴마다 처음부터, 아니면 끝 프레임에 머문다. 길이가 0 이면 0)
	/// 클립이나 프레임이 없으면 None.
	//==============================================================================
	pub fn get_frame_index(&self, time: f32) -> Option<usize> {
		let clip = self.clip.as_ref()?;
		let frame_count = clip.get_frame_count();
		if frame_count == 0 {
			return None;
		}
		let duration = clip.get_duration();
		if duration <= 0.0 {
			return Some(0);
		}
		let normalized_time = time / duration;
		let cycle_time = if self.is_loop() {
			normalized_time - normalized_time.floor()
		}
		else {
			normalized_time.clamp(0.0, 1.0)
		};
		Some(((cycle_time * frame_count as f32).floor() as usize).min(frame_count - 1))
	}
}

impl Default for AnimationState {
	fn default() -> AnimationState {
		AnimationState::new()
	}
}

//==============================================================================
/// 애니메이션 전이. (조건이 맞으면 대상 상태로 넘어가는 상태 사이의 연결, 상태는 Animator 상태 목록 번호)
/// - from 이 None 이면 어느 상태에서나(Any State) 넘어간다. to 가 None 이거나 상태 목록 밖이면 쓰지 않는다.
/// - 종료 시각(exit_time)은 나가는 상태의 정규화 시간이다. 정해 두면 그 시각을 지나는 갱신에서만 넘어갈 수 있다.
///   되풀이하는 상태에서 1 보다 작은 종료 시각은 바퀴마다 본다. (0.5 면 1.5, 2.5 에서도) 그 밖은 한 번만 본다.
/// - 조건은 모두 맞아야 한다. 종료 시각도 조건도 없는 전이는 쓰지 않는다.
/// - 섞는 시간(duration, 초)이 0 보다 크면 그동안 두 상태가 함께 흐르며 가중치가 선형으로 옮겨 간다. (크로스페이드)
/// - 어느 상태에서나 넘어가는 전이가 지금 상태로 가는 경우는 can_transition_to_self 가 참일 때만 탄다. (기본 참, 다시 처음부터 튼다)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct AnimationTransition {
	from: Option<usize>,
	to: Option<usize>,
	exit_time: Option<f32>,
	duration: f32,
	can_transition_to_self: bool,
	conditions: Vec<AnimationCondition>,
}

impl AnimationTransition {
	//==============================================================================
	/// 생성. (어느 상태에서나, 들어가는 상태 없음, 종료 시각 없음, 섞는 시간 0, 지금 상태로도 감)
	//==============================================================================
	pub fn new() -> AnimationTransition {
		AnimationTransition {
			from: None,
			to: None,
			exit_time: None,
			duration: 0.0,
			can_transition_to_self: true,
			conditions: Vec::new(),
		}
	}

	//==============================================================================
	/// 나가는 상태 설정. (Animator::add_state 가 돌려준 번호)
	///
	/// # 인자
	/// - `from`: 나가는 상태의 번호. (None 이면 어느 상태에서나, 기본 None)
	//==============================================================================
	pub fn set_from(&mut self, from: Option<usize>) {
		self.from = from;
	}

	//==============================================================================
	/// 나가는 상태 반환.
	///
	/// # 돌려줌
	/// 나가는 상태의 번호. 어느 상태에서나 넘어가면 None.
	//==============================================================================
	pub fn get_from(&self) -> Option<usize> {
		self.from
	}

	//==============================================================================
	/// 들어가는 상태 설정. (Animator::add_state 가 돌려준 번호)
	///
	/// # 인자
	/// - `to`: 들어가는 상태의 번호. (None 이면 비우고, 빈 전이는 쓰지 않는다)
	//==============================================================================
	pub fn set_to(&mut self, to: Option<usize>) {
		self.to = to;
	}

	//==============================================================================
	/// 들어가는 상태 반환.
	///
	/// # 돌려줌
	/// 들어가는 상태의 번호. 정하지 않았으면 None.
	//==============================================================================
	pub fn get_to(&self) -> Option<usize> {
		self.to
	}

	//==============================================================================
	/// 종료 시각 설정.
	///
	/// # 인자
	/// - `exit_time`: 나가는 상태의 정규화 시간. (1 이면 한 바퀴 끝, None 이면 종료 시각 없이 조건만 본다, 기본 None)
	//==============================================================================
	pub fn set_exit_time(&mut self, exit_time: Option<f32>) {
		self.exit_time = exit_time;
	}

	//==============================================================================
	/// 종료 시각 반환.
	///
	/// # 돌려줌
	/// 종료 시각. (정규화 시간, 없으면 None)
	//==============================================================================
	pub fn get_exit_time(&self) -> Option<f32> {
		self.exit_time
	}

	//==============================================================================
	/// 섞는 시간 설정.
	///
	/// # 인자
	/// - `duration`: 두 상태를 섞는 시간. (초, 0 이하이면 바로 넘어간다, 기본 0)
	//==============================================================================
	pub fn set_duration(&mut self, duration: f32) {
		self.duration = duration;
	}

	//==============================================================================
	/// 섞는 시간 반환.
	///
	/// # 돌려줌
	/// 섞는 시간. (초)
	//==============================================================================
	pub fn get_duration(&self) -> f32 {
		self.duration
	}

	//==============================================================================
	/// 지금 상태로 가는 것을 허락할지 설정.
	///
	/// # 인자
	/// - `can_transition_to_self`: 참이면 어느 상태에서나 넘어가는 전이가 지금 상태로도 간다. (기본 참, from 이 정해진 전이에는 쓰지 않는다)
	//==============================================================================
	pub fn set_can_transition_to_self(&mut self, can_transition_to_self: bool) {
		self.can_transition_to_self = can_transition_to_self;
	}

	//==============================================================================
	/// 지금 상태로 가는 것을 허락하는지 반환.
	///
	/// # 돌려줌
	/// 어느 상태에서나 넘어가는 전이가 지금 상태로도 가면 참.
	//==============================================================================
	pub fn can_transition_to_self(&self) -> bool {
		self.can_transition_to_self
	}

	//==============================================================================
	/// 조건 추가.
	///
	/// # 인자
	/// - `parameter_name`: 견줄 매개변수 이름.
	/// - `mode`: 견주는 법.
	/// - `threshold`: 기준 값. (Greater, Less, Equals, NotEqual 에서 쓴다, 원본 기본 0)
	//==============================================================================
	pub fn add_condition(&mut self, parameter_name: &str, mode: AnimationConditionMode, threshold: f32) {
		self.conditions.push(AnimationCondition {
			parameter_name: parameter_name.to_string(),
			mode,
			threshold,
		});
	}

	//==============================================================================
	/// 조건 목록 반환.
	///
	/// # 돌려줌
	/// 넣은 차례의 조건 목록.
	//==============================================================================
	pub fn get_conditions(&self) -> &[AnimationCondition] {
		&self.conditions
	}

	//==============================================================================
	/// 종료 시각을 지났는지 반환.
	///
	/// # 인자
	/// - `state`: 나가는 상태.
	/// - `previous_time`: 이번 갱신 앞의 상태 시간. (초)
	/// - `current_time`: 이번 갱신 뒤의 상태 시간. (초)
	///
	/// # 돌려줌
	/// 종료 시각이 없거나, 이번 갱신에서 종료 시각을 지났으면 참. (길이가 0 인 상태는 들어온 뒤 첫 갱신에서 지난다)
	//==============================================================================
	pub fn is_exit_time_reached(&self, state: &AnimationState, previous_time: f32, current_time: f32) -> bool {
		let exit_time = match self.exit_time {
			Some(exit_time) => {
				exit_time
			}
			None => {
				return true;
			}
		};
		let duration = state.get_duration();
		if duration <= 0.0 {
			return previous_time <= 0.0 && current_time > 0.0;
		}
		let previous_normalized_time = previous_time / duration;
		let current_normalized_time = current_time / duration;
		if current_normalized_time <= previous_normalized_time {
			return false;
		}

		// 되풀이하는 상태에서 1 보다 작은 종료 시각은 바퀴마다 본다.
		let mut crossing_time = exit_time;
		if state.is_loop() && exit_time < 1.0 {
			crossing_time = exit_time + (previous_normalized_time - exit_time).ceil().max(0.0);
		}
		previous_normalized_time <= crossing_time && crossing_time < current_normalized_time
	}
}

impl Default for AnimationTransition {
	fn default() -> AnimationTransition {
		AnimationTransition::new()
	}
}

//==============================================================================
/// 애니메이터. (상태 기계로 스프라이트 클립을 고르고 섞는다)
/// - 매개변수는 실수(set_float), 정수(set_integer), 참 거짓(set_bool), 트리거(set_trigger) 넷이다. 같은 이름을 다른 종류로 넣으면 종류째 바꾼다.
///   트리거는 그 트리거를 조건으로 쓴 전이가 타면 풀린다. (reset_trigger 로 직접 풀 수도 있다)
/// - 섞는 중의 가중치: 다음 상태가 get_transition_weight(섞은 시간 / 섞는 시간), 지금 상태가 1 - 그 값이다.
/// - 고른 프레임은 get_current_frame, get_next_frame 으로 꺼낸다. (클립 프레임 목록의 영역)
///
/// # 예제
/// 속도로 걷기와 달리기를 오가는 애니메이터
/// ```rust
/// use vanilla::animation::animator::{AnimationConditionMode, AnimationState, AnimationTransition, Animator};
/// use vanilla::base::rect::Rect;
/// use vanilla::resource::animationclip::AnimationClip;
///
/// let mut walk_clip = AnimationClip::new();
/// walk_clip.set_frames(vec![Rect::create(0.0, 0.0, 32.0, 32.0), Rect::create(32.0, 0.0, 32.0, 32.0)]);
/// walk_clip.set_duration(1.0);
/// walk_clip.set_loop(true);
/// let mut run_clip = AnimationClip::new();
/// run_clip.set_frames(vec![Rect::create(0.0, 32.0, 32.0, 32.0), Rect::create(32.0, 32.0, 32.0, 32.0)]);
/// run_clip.set_duration(0.5);
/// run_clip.set_loop(true);
///
/// let mut animator = Animator::new();
/// let mut walk_state = AnimationState::new();
/// walk_state.set_clip(Some(walk_clip));
/// let mut run_state = AnimationState::new();
/// run_state.set_clip(Some(run_clip));
/// let walk_index = animator.add_state(walk_state);
/// let run_index = animator.add_state(run_state);
/// let mut to_run = AnimationTransition::new();
/// to_run.set_from(Some(walk_index));
/// to_run.set_to(Some(run_index));
/// to_run.set_duration(0.25);
/// to_run.add_condition("speed", AnimationConditionMode::Greater, 3.0);
/// animator.add_transition(to_run);
/// let mut to_walk = AnimationTransition::new();
/// to_walk.set_from(Some(run_index));
/// to_walk.set_to(Some(walk_index));
/// to_walk.add_condition("speed", AnimationConditionMode::Less, 1.0);
/// animator.add_transition(to_walk);
///
/// animator.set_float("speed", 5.0);
/// animator.tick(0.125);
/// println!("{:?} {:?} {}", animator.get_current_state_index(), animator.get_next_state_index(), animator.get_transition_weight());
/// animator.tick(0.125);
/// println!("{:?} {}", animator.get_next_state_index(), animator.get_transition_weight());
/// animator.tick(0.125);
/// let frame = animator.get_current_frame().expect("frame");
/// println!("{:?} {} {}", animator.get_current_state_index(), animator.is_in_transition(), frame.position.y);
/// ```
//==============================================================================
pub struct Animator {
	states: Vec<AnimationState>,
	transitions: Vec<AnimationTransition>,
	parameters: HashMap<String, AnimatorParameter>,
	current_state_index: Option<usize>,
	/// 지금 상태 시간. (초, 속도를 곱해 쌓은 것)
	current_time: f32,
	next_state_index: Option<usize>,
	/// 들어가는 상태 시간. (초)
	next_time: f32,
	/// 섞은 시간. (초)
	transition_elapsed: f32,
	/// 섞는 시간. (초)
	transition_duration: f32,
}

impl Animator {
	//==============================================================================
	/// 생성. (상태 없음)
	//==============================================================================
	pub fn new() -> Animator {
		Animator {
			states: Vec::new(),
			transitions: Vec::new(),
			parameters: HashMap::new(),
			current_state_index: None,
			current_time: 0.0,
			next_state_index: None,
			next_time: 0.0,
			transition_elapsed: 0.0,
			transition_duration: 0.0,
		}
	}

	//==============================================================================
	/// 갱신.
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임부터의 시간. (초, 상태마다 속도를 곱해 흘린다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		let current_state_index = match self.current_state_index {
			Some(current_state_index) => {
				current_state_index
			}
			None => {
				return;
			}
		};
		let current_speed = self.states[current_state_index].get_speed();
		let previous_time;
		if let Some(next_state_index) = self.next_state_index {
			// 섞는 중: 두 상태가 함께 흐르고, 섞는 시간을 다 채우면 들어가는 상태가 지금 상태가 된다.
			let next_speed = self.states[next_state_index].get_speed();
			self.current_time += time_delta * current_speed;
			previous_time = self.next_time;
			self.next_time += time_delta * next_speed;
			self.transition_elapsed += time_delta;
			if self.transition_elapsed < self.transition_duration {
				return;
			}
			self.current_state_index = Some(next_state_index);
			self.current_time = self.next_time;
			self.next_state_index = None;
			self.next_time = 0.0;
			self.transition_elapsed = 0.0;
			self.transition_duration = 0.0;
		}
		else {
			previous_time = self.current_time;
			self.current_time += time_delta * current_speed;
		}

		// 탈 전이 찾기.
		if let Some(transition_index) = self.find_transition(previous_time) {
			self.start_transition(transition_index);
		}
	}

	//==============================================================================
	/// 상태 바로 틀기. (섞지 않고 처음부터)
	///
	/// # 인자
	/// - `state_index`: 틀 상태의 번호. (add_state 가 돌려준 것, 목록 밖이면 무시한다)
	//==============================================================================
	pub fn play(&mut self, state_index: usize) {
		if state_index >= self.states.len() {
			return;
		}
		self.current_state_index = Some(state_index);
		self.current_time = 0.0;
		self.next_state_index = None;
		self.next_time = 0.0;
		self.transition_elapsed = 0.0;
		self.transition_duration = 0.0;
	}

	//==============================================================================
	/// 탈 전이 찾기.
	///
	/// # 인자
	/// - `previous_time`: 이번 갱신 앞의 지금 상태 시간. (초, 종료 시각을 지났는지 볼 때 쓴다)
	///
	/// # 돌려줌
	/// 처음 맞는 전이의 번호. (어느 상태에서나 넘어가는 전이를 먼저, 그다음 지금 상태에서 나가는 전이를 넣은 순서대로 본다)
	/// 섞는 중이거나 없으면 None.
	//==============================================================================
	pub fn find_transition(&self, previous_time: f32) -> Option<usize> {
		let current_state_index = self.current_state_index?;
		if self.next_state_index.is_some() {
			return None;
		}
		let current_state = &self.states[current_state_index];
		for is_any_state_pass in [true, false] {
			for (transition_index, transition) in self.transitions.iter().enumerate() {
				let to = match transition.get_to() {
					Some(to) if to < self.states.len() => {
						to
					}
					_ => {
						continue;
					}
				};
				if is_any_state_pass {
					if transition.get_from().is_some() {
						continue;
					}
					if to == current_state_index && !transition.can_transition_to_self() {
						continue;
					}
				}
				else if transition.get_from() != Some(current_state_index) {
					continue;
				}

				// 종료 시각도 조건도 없는 전이는 쓰지 않는다.
				if transition.get_exit_time().is_none() && transition.get_conditions().is_empty() {
					continue;
				}
				if !transition.is_exit_time_reached(current_state, previous_time, self.current_time) {
					continue;
				}
				if transition.get_conditions().iter().all(|condition| self.is_condition_satisfied(condition)) {
					return Some(transition_index);
				}
			}
		}
		None
	}

	//==============================================================================
	/// 전이 타기. (조건과 종료 시각은 보지 않는다)
	///
	/// # 인자
	/// - `transition_index`: 탈 전이의 번호. (add_transition 이 돌려준 것, 목록 밖이거나 들어가는 상태가 목록 밖이면 무시한다)
	//==============================================================================
	pub fn start_transition(&mut self, transition_index: usize) {
		let transition = match self.transitions.get(transition_index) {
			Some(transition) => {
				transition
			}
			None => {
				return;
			}
		};
		let to = match transition.get_to() {
			Some(to) if to < self.states.len() => {
				to
			}
			_ => {
				return;
			}
		};
		let duration = transition.get_duration();

		// 조건으로 쓴 트리거는 탄 전이가 쓴 것으로 치고 푼다.
		for condition in transition.conditions.iter() {
			if let Some(AnimatorParameter::Trigger(is_set)) = self.parameters.get_mut(&condition.parameter_name) {
				*is_set = false;
			}
		}

		if duration > 0.0 {
			self.next_state_index = Some(to);
			self.next_time = 0.0;
			self.transition_elapsed = 0.0;
			self.transition_duration = duration;
		}
		else {
			self.play(to);
		}
	}

	//==============================================================================
	/// 조건이 맞는지 반환.
	///
	/// # 인자
	/// - `condition`: 볼 조건. (AnimationTransition::get_conditions 의 요소)
	///
	/// # 돌려줌
	/// 지금 매개변수로 조건이 맞으면 참. (매개변수가 없거나 견줄 수 없는 종류면 거짓)
	//==============================================================================
	pub fn is_condition_satisfied(&self, condition: &AnimationCondition) -> bool {
		let parameter = match self.parameters.get(&condition.parameter_name) {
			Some(parameter) => {
				*parameter
			}
			None => {
				return false;
			}
		};
		let number_value = match parameter {
			AnimatorParameter::Float(value) => {
				Some(value)
			}
			AnimatorParameter::Int(value) => {
				Some(value as f32)
			}
			_ => {
				None
			}
		};
		match condition.mode {
			AnimationConditionMode::If => {
				matches!(parameter, AnimatorParameter::Bool(true) | AnimatorParameter::Trigger(true))
			}
			AnimationConditionMode::IfNot => {
				matches!(parameter, AnimatorParameter::Bool(false))
			}
			AnimationConditionMode::Greater => {
				number_value.is_some_and(|value| value > condition.threshold)
			}
			AnimationConditionMode::Less => {
				number_value.is_some_and(|value| value < condition.threshold)
			}
			AnimationConditionMode::Equals => {
				number_value.is_some_and(|value| value == condition.threshold)
			}
			AnimationConditionMode::NotEqual => {
				number_value.is_some_and(|value| value != condition.threshold)
			}
		}
	}

	//==============================================================================
	/// 실수 매개변수 설정.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	/// - `value`: 값. (같은 이름이 있으면 종류째 바꾼다)
	//==============================================================================
	pub fn set_float(&mut self, name: &str, value: f32) {
		self.parameters.insert(name.to_string(), AnimatorParameter::Float(value));
	}

	//==============================================================================
	/// 실수 매개변수 반환.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	///
	/// # 돌려줌
	/// 값. (없거나 실수가 아니면 0)
	//==============================================================================
	pub fn get_float(&self, name: &str) -> f32 {
		match self.parameters.get(name) {
			Some(AnimatorParameter::Float(value)) => {
				*value
			}
			_ => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 정수 매개변수 설정.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	/// - `value`: 값. (같은 이름이 있으면 종류째 바꾼다)
	//==============================================================================
	pub fn set_integer(&mut self, name: &str, value: i32) {
		self.parameters.insert(name.to_string(), AnimatorParameter::Int(value));
	}

	//==============================================================================
	/// 정수 매개변수 반환.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	///
	/// # 돌려줌
	/// 값. (없거나 정수가 아니면 0)
	//==============================================================================
	pub fn get_integer(&self, name: &str) -> i32 {
		match self.parameters.get(name) {
			Some(AnimatorParameter::Int(value)) => {
				*value
			}
			_ => {
				0
			}
		}
	}

	//==============================================================================
	/// 참 거짓 매개변수 설정.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	/// - `value`: 값. (같은 이름이 있으면 종류째 바꾼다)
	//==============================================================================
	pub fn set_bool(&mut self, name: &str, value: bool) {
		self.parameters.insert(name.to_string(), AnimatorParameter::Bool(value));
	}

	//==============================================================================
	/// 참 거짓 매개변수 반환.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	///
	/// # 돌려줌
	/// 값. (없거나 참 거짓이 아니면 거짓)
	//==============================================================================
	pub fn get_bool(&self, name: &str) -> bool {
		matches!(self.parameters.get(name), Some(AnimatorParameter::Bool(true)))
	}

	//==============================================================================
	/// 트리거 걸기. (그 트리거를 조건으로 쓴 전이가 타면 풀린다)
	///
	/// # 인자
	/// - `name`: 매개변수 이름. (같은 이름이 있으면 종류째 바꾼다)
	//==============================================================================
	pub fn set_trigger(&mut self, name: &str) {
		self.parameters.insert(name.to_string(), AnimatorParameter::Trigger(true));
	}

	//==============================================================================
	/// 트리거 풀기.
	///
	/// # 인자
	/// - `name`: 매개변수 이름. (같은 이름이 있으면 종류째 바꾼다)
	//==============================================================================
	pub fn reset_trigger(&mut self, name: &str) {
		self.parameters.insert(name.to_string(), AnimatorParameter::Trigger(false));
	}

	//==============================================================================
	/// 트리거가 걸려 있는지 반환.
	///
	/// # 인자
	/// - `name`: 매개변수 이름.
	///
	/// # 돌려줌
	/// 걸려 있으면 참. (없거나 트리거가 아니면 거짓)
	//==============================================================================
	pub fn get_trigger(&self, name: &str) -> bool {
		matches!(self.parameters.get(name), Some(AnimatorParameter::Trigger(true)))
	}

	//==============================================================================
	/// 상태 더하기. (처음 더한 상태에서 시작한다)
	///
	/// # 인자
	/// - `state`: 더할 상태.
	///
	/// # 돌려줌
	/// 더한 상태의 번호. (0 부터)
	//==============================================================================
	pub fn add_state(&mut self, state: AnimationState) -> usize {
		self.states.push(state);
		let state_index = self.states.len() - 1;
		if self.current_state_index.is_none() {
			self.play(state_index);
		}
		state_index
	}

	//==============================================================================
	/// 상태 목록 반환.
	///
	/// # 돌려줌
	/// 상태 목록.
	//==============================================================================
	pub fn get_states(&self) -> &[AnimationState] {
		&self.states
	}

	//==============================================================================
	/// 전이 더하기.
	///
	/// # 인자
	/// - `transition`: 더할 전이.
	///
	/// # 돌려줌
	/// 더한 전이의 번호. (0 부터)
	//==============================================================================
	pub fn add_transition(&mut self, transition: AnimationTransition) -> usize {
		self.transitions.push(transition);
		self.transitions.len() - 1
	}

	//==============================================================================
	/// 전이 목록 반환.
	///
	/// # 돌려줌
	/// 전이 목록.
	//==============================================================================
	pub fn get_transitions(&self) -> &[AnimationTransition] {
		&self.transitions
	}

	//==============================================================================
	/// 지금 상태 반환.
	///
	/// # 돌려줌
	/// 지금 상태의 번호. (섞는 중이면 나가는 상태) 상태가 없으면 None.
	//==============================================================================
	pub fn get_current_state_index(&self) -> Option<usize> {
		self.current_state_index
	}

	//==============================================================================
	/// 지금 상태의 정규화 시간 반환.
	///
	/// # 돌려줌
	/// 지금 상태 시간을 한 바퀴 길이로 나눈 것. (상태가 없으면 0)
	//==============================================================================
	pub fn get_current_normalized_time(&self) -> f32 {
		match self.current_state_index {
			Some(current_state_index) => {
				self.states[current_state_index].get_normalized_time(self.current_time)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 지금 상태의 프레임 반환.
	///
	/// # 돌려줌
	/// 지금 상태 시간에 보일 클립 프레임의 영역. 상태나 클립이나 프레임이 없으면 None.
	//==============================================================================
	pub fn get_current_frame(&self) -> Option<&Rect> {
		let current_state = &self.states[self.current_state_index?];
		let frame_index = current_state.get_frame_index(self.current_time)?;
		current_state.get_clip()?.get_frame(frame_index)
	}

	//==============================================================================
	/// 섞는 중인지 반환.
	///
	/// # 돌려줌
	/// 두 상태를 섞는 중이면 참.
	//==============================================================================
	pub fn is_in_transition(&self) -> bool {
		self.next_state_index.is_some()
	}

	//==============================================================================
	/// 섞으며 들어가는 상태 반환.
	///
	/// # 돌려줌
	/// 들어가는 상태의 번호. 섞는 중이 아니면 None.
	//==============================================================================
	pub fn get_next_state_index(&self) -> Option<usize> {
		self.next_state_index
	}

	//==============================================================================
	/// 들어가는 상태의 정규화 시간 반환.
	///
	/// # 돌려줌
	/// 들어가는 상태 시간을 한 바퀴 길이로 나눈 것. (섞는 중이 아니면 0)
	//==============================================================================
	pub fn get_next_normalized_time(&self) -> f32 {
		match self.next_state_index {
			Some(next_state_index) => {
				self.states[next_state_index].get_normalized_time(self.next_time)
			}
			None => {
				0.0
			}
		}
	}

	//==============================================================================
	/// 들어가는 상태의 프레임 반환.
	///
	/// # 돌려줌
	/// 들어가는 상태 시간에 보일 클립 프레임의 영역. 섞는 중이 아니거나 클립이나 프레임이 없으면 None.
	//==============================================================================
	pub fn get_next_frame(&self) -> Option<&Rect> {
		let next_state = &self.states[self.next_state_index?];
		let frame_index = next_state.get_frame_index(self.next_time)?;
		next_state.get_clip()?.get_frame(frame_index)
	}

	//==============================================================================
	/// 들어가는 상태의 가중치 반환. (크로스페이드)
	///
	/// # 돌려줌
	/// 섞은 시간 / 섞는 시간. (0 ~ 1, 지금 상태의 가중치는 1 - 이 값, 섞는 중이 아니면 0)
	//==============================================================================
	pub fn get_transition_weight(&self) -> f32 {
		if self.next_state_index.is_none() || self.transition_duration <= 0.0 {
			return 0.0;
		}
		(self.transition_elapsed / self.transition_duration).clamp(0.0, 1.0)
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

	//==============================================================================
	/// 시험용 클립. (프레임 수만큼 가로로 늘어선 32 칸, 길이, 되풀이)
	//==============================================================================
	fn create_clip(frame_count: usize, duration: f32, is_loop: bool) -> AnimationClip {
		let mut clip = AnimationClip::new();
		clip.set_frames((0..frame_count).map(|frame_index| Rect::create(frame_index as f32 * 32.0, 0.0, 32.0, 32.0)).collect());
		clip.set_duration(duration);
		clip.set_loop(is_loop);
		clip
	}

	//==============================================================================
	/// 시험용 상태. (클립 하나)
	//==============================================================================
	fn create_state(frame_count: usize, duration: f32, is_loop: bool) -> AnimationState {
		let mut state = AnimationState::new();
		state.set_clip(Some(create_clip(frame_count, duration, is_loop)));
		state
	}

	//==============================================================================
	/// 시험용 전이.
	//==============================================================================
	fn create_transition(from: Option<usize>, to: usize, exit_time: Option<f32>, duration: f32) -> AnimationTransition {
		let mut transition = AnimationTransition::new();
		transition.set_from(from);
		transition.set_to(Some(to));
		transition.set_exit_time(exit_time);
		transition.set_duration(duration);
		transition
	}

	//==============================================================================
	/// 매개변수는 종류별로 넣고 꺼낸다. (다른 종류로 넣으면 종류째 바뀌고, 없거나 종류가 다르면 기본값)
	//==============================================================================
	#[test]
	fn parameters_are_typed() {
		let mut animator = Animator::new();
		animator.set_float("speed", 1.5);
		animator.set_integer("combo", 3);
		animator.set_bool("isGrounded", true);
		animator.set_trigger("jump");
		assert_eq!(animator.get_float("speed"), 1.5);
		assert_eq!(animator.get_integer("combo"), 3);
		assert!(animator.get_bool("isGrounded"));
		assert!(animator.get_trigger("jump"));
		assert_eq!(animator.get_float("combo"), 0.0);
		assert!(!animator.get_bool("missing"));
		animator.set_bool("speed", false);
		assert_eq!(animator.get_float("speed"), 0.0);
		assert!(!animator.get_bool("speed"));
		animator.reset_trigger("jump");
		assert!(!animator.get_trigger("jump"));
	}

	//==============================================================================
	/// 조건은 모두 맞아야 넘어간다. (실수 greater 와 참 거짓 if)
	//==============================================================================
	#[test]
	fn transition_requires_all_conditions() {
		let mut animator = Animator::new();
		let idle_index = animator.add_state(create_state(2, 1.0, true));
		let run_index = animator.add_state(create_state(2, 1.0, true));
		let mut transition = create_transition(Some(idle_index), run_index, None, 0.0);
		transition.add_condition("speed", AnimationConditionMode::Greater, 0.5);
		transition.add_condition("isGrounded", AnimationConditionMode::If, 0.0);
		animator.add_transition(transition);
		assert_eq!(animator.get_current_state_index(), Some(idle_index));
		animator.set_float("speed", 1.0);
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(idle_index));
		animator.set_bool("isGrounded", true);
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(run_index));
		assert_eq!(animator.get_current_normalized_time(), 0.0);
	}

	//==============================================================================
	/// 정수 조건. (equals, notEqual, less)
	//==============================================================================
	#[test]
	fn integer_conditions_compare_threshold() {
		let mut animator = Animator::new();
		animator.set_integer("combo", 2);
		let mut condition = AnimationCondition { parameter_name: String::from("combo"), mode: AnimationConditionMode::Equals, threshold: 2.0 };
		assert!(animator.is_condition_satisfied(&condition));
		condition.mode = AnimationConditionMode::NotEqual;
		assert!(!animator.is_condition_satisfied(&condition));
		condition.mode = AnimationConditionMode::Less;
		condition.threshold = 3.0;
		assert!(animator.is_condition_satisfied(&condition));
		condition.mode = AnimationConditionMode::IfNot;
		assert!(!animator.is_condition_satisfied(&condition));
		animator.set_bool("combo", false);
		assert!(animator.is_condition_satisfied(&condition));
	}

	//==============================================================================
	/// 종료 시각은 지나는 갱신에서만 맞고, 되풀이하는 상태의 1 보다 작은 종료 시각은 바퀴마다 맞는다.
	//==============================================================================
	#[test]
	fn exit_time_is_checked_when_crossed() {
		let looping_state = create_state(4, 1.0, true);
		let once_state = create_state(4, 1.0, false);
		let empty_state = AnimationState::new();
		let transition = create_transition(Some(0), 1, Some(0.5), 0.0);
		assert!(!transition.is_exit_time_reached(&looping_state, 0.25, 0.5));
		assert!(transition.is_exit_time_reached(&looping_state, 0.5, 0.75));
		assert!(!transition.is_exit_time_reached(&looping_state, 0.75, 1.0));
		assert!(transition.is_exit_time_reached(&looping_state, 1.25, 1.75));
		assert!(!transition.is_exit_time_reached(&once_state, 1.25, 1.75));
		assert!(transition.is_exit_time_reached(&once_state, 0.25, 0.75));
		let late_transition = create_transition(Some(0), 1, Some(2.5), 0.0);
		assert!(late_transition.is_exit_time_reached(&looping_state, 2.25, 2.75));
		assert!(!late_transition.is_exit_time_reached(&looping_state, 3.25, 3.75));
		assert!(transition.is_exit_time_reached(&empty_state, 0.0, 0.125));
		assert!(!transition.is_exit_time_reached(&empty_state, 0.125, 0.25));
		assert!(!transition.is_exit_time_reached(&looping_state, 0.75, 0.75));
	}

	//==============================================================================
	/// 종료 시각을 지날 때 조건이 맞지 않으면 다음 바퀴의 그 시각까지 기다린다. (그 시각에 꼭 닿은 갱신은 다음 갱신에서 지난 것으로 친다)
	//==============================================================================
	#[test]
	fn exit_time_transition_waits_for_crossing() {
		let mut animator = Animator::new();
		let attack_index = animator.add_state(create_state(4, 0.5, true));
		let idle_index = animator.add_state(create_state(2, 1.0, true));
		let mut transition = create_transition(Some(attack_index), idle_index, Some(0.5), 0.0);
		transition.add_condition("isDone", AnimationConditionMode::If, 0.0);
		animator.add_transition(transition);
		animator.tick(0.375);
		assert_eq!(animator.get_current_state_index(), Some(attack_index));
		animator.set_bool("isDone", true);
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(attack_index));
		animator.tick(0.25);
		assert_eq!(animator.get_current_normalized_time(), 1.5);
		assert_eq!(animator.get_current_state_index(), Some(attack_index));
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(idle_index));
	}

	//==============================================================================
	/// 트리거는 그 트리거를 쓴 전이가 타면 풀린다. (조건이 다 맞지 않아 타지 않으면 남는다)
	//==============================================================================
	#[test]
	fn trigger_is_consumed_by_transition() {
		let mut animator = Animator::new();
		let idle_index = animator.add_state(create_state(2, 1.0, true));
		let jump_index = animator.add_state(create_state(2, 1.0, false));
		let mut transition = create_transition(Some(idle_index), jump_index, None, 0.0);
		transition.add_condition("jump", AnimationConditionMode::If, 0.0);
		transition.add_condition("isGrounded", AnimationConditionMode::If, 0.0);
		animator.add_transition(transition);
		animator.set_trigger("jump");
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(idle_index));
		assert!(animator.get_trigger("jump"));
		animator.set_bool("isGrounded", true);
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(jump_index));
		assert!(!animator.get_trigger("jump"));
	}

	//==============================================================================
	/// 크로스페이드: 섞는 동안 가중치가 선형으로 오르고 두 상태가 함께 흐르며, 다 채우면 들어가는 상태가 지금 상태가 된다.
	//==============================================================================
	#[test]
	fn crossfade_weight_moves_linearly() {
		let mut animator = Animator::new();
		let walk_index = animator.add_state(create_state(4, 1.0, true));
		let mut run_state = create_state(4, 0.5, true);
		run_state.set_speed(2.0);
		let run_index = animator.add_state(run_state);
		let mut transition = create_transition(Some(walk_index), run_index, None, 0.5);
		transition.add_condition("speed", AnimationConditionMode::Greater, 3.0);
		animator.add_transition(transition);
		animator.set_float("speed", 5.0);
		animator.tick(0.25);
		assert!(animator.is_in_transition());
		assert_eq!(animator.get_next_state_index(), Some(run_index));
		assert_eq!(animator.get_transition_weight(), 0.0);
		assert_eq!(animator.get_current_normalized_time(), 0.25);
		animator.tick(0.125);
		assert_eq!(animator.get_transition_weight(), 0.25);
		assert_eq!(animator.get_next_normalized_time(), 0.5);
		assert_eq!(animator.get_current_normalized_time(), 0.375);
		assert_eq!(animator.get_next_frame().map(|frame| frame.position.x), Some(64.0));
		animator.tick(0.25);
		assert_eq!(animator.get_transition_weight(), 0.75);
		animator.tick(0.125);
		assert!(!animator.is_in_transition());
		assert_eq!(animator.get_current_state_index(), Some(run_index));
		assert_eq!(animator.get_current_normalized_time(), 2.0);
		assert_eq!(animator.get_transition_weight(), 0.0);
	}

	//==============================================================================
	/// 어느 상태에서나 넘어가는 전이는 먼저 보고, 지금 상태로 가는 것은 can_transition_to_self 로 막는다.
	//==============================================================================
	#[test]
	fn any_state_transition_takes_priority() {
		let mut animator = Animator::new();
		let idle_index = animator.add_state(create_state(2, 1.0, true));
		let run_index = animator.add_state(create_state(2, 1.0, true));
		let hit_index = animator.add_state(create_state(2, 1.0, false));
		let mut run_transition = create_transition(Some(idle_index), run_index, None, 0.0);
		run_transition.add_condition("speed", AnimationConditionMode::Greater, 0.0);
		animator.add_transition(run_transition);
		let mut hit_transition = create_transition(None, hit_index, None, 0.0);
		hit_transition.add_condition("isHit", AnimationConditionMode::If, 0.0);
		hit_transition.set_can_transition_to_self(false);
		animator.add_transition(hit_transition);
		animator.set_float("speed", 1.0);
		animator.set_bool("isHit", true);
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(hit_index));
		animator.tick(0.125);
		assert_eq!(animator.get_current_state_index(), Some(hit_index));
		assert_eq!(animator.get_current_normalized_time(), 0.125);
	}

	//==============================================================================
	/// 프레임은 상태 시간으로 고른다. (되풀이하면 바퀴마다 처음부터, 아니면 끝 프레임에 머물고, 상태의 되풀이가 클립을 덮는다)
	//==============================================================================
	#[test]
	fn frame_follows_state_time() {
		let mut state = create_state(4, 1.0, false);
		assert_eq!(state.get_frame_index(0.0), Some(0));
		assert_eq!(state.get_frame_index(0.5), Some(2));
		assert_eq!(state.get_frame_index(1.5), Some(3));
		state.set_loop(Some(true));
		assert_eq!(state.get_frame_index(1.25), Some(1));
		assert_eq!(AnimationState::new().get_frame_index(0.5), None);
		let mut animator = Animator::new();
		assert!(animator.get_current_frame().is_none());
		animator.add_state(state);
		animator.tick(0.5);
		assert_eq!(animator.get_current_frame().map(|frame| frame.position.x), Some(64.0));
		animator.play(5);
		assert_eq!(animator.get_current_normalized_time(), 0.5);
		animator.play(0);
		assert_eq!(animator.get_current_normalized_time(), 0.0);
	}

	//==============================================================================
	/// 원본과 같은 시나리오로 같은 상태 차례, 가중치, 정규화 시간, 프레임, 트리거를 낸다.
	/// - 기대 기록은 원본 animator.js 로 같은 시나리오를 돌려 뽑은 것이다. (줄마다 갱신 차례, 지금 상태, 들어가는 상태, 가중치, 두 정규화 시간, 두 프레임 자리, 트리거)
	/// - 섞기, 종료 시각(꼭 닿은 갱신은 다음 갱신에서 셈), 어느 상태에서나 넘어가는 트리거와 지금 상태로 가지 않기, 남은 트리거, 정수 equals, 속도를 거친다.
	//==============================================================================
	#[test]
	fn same_trace_as_original() {
		let mut animator = Animator::new();
		let mut walk_state = create_state(4, 0.5, true);
		walk_state.set_speed(2.0);
		for (state_index, mut state) in [create_state(4, 1.0, true), walk_state, create_state(4, 0.5, false), create_state(2, 0.25, false)].into_iter().enumerate() {
			let mut clip = state.get_clip().expect("clip").clone();
			let frames = clip.get_frames().iter().map(|frame| Rect::create(frame.position.x, state_index as f32 * 32.0, frame.size.x, frame.size.y)).collect();
			clip.set_frames(frames);
			state.set_clip(Some(clip));
			animator.add_state(state);
		}
		let mut to_walk = create_transition(Some(0), 1, None, 0.25);
		to_walk.add_condition("speed", AnimationConditionMode::Greater, 0.5);
		animator.add_transition(to_walk);
		let mut to_idle = create_transition(Some(1), 0, None, 0.125);
		to_idle.add_condition("speed", AnimationConditionMode::Less, 0.5);
		animator.add_transition(to_idle);
		let mut to_jump = create_transition(None, 2, None, 0.0);
		to_jump.add_condition("jump", AnimationConditionMode::If, 0.0);
		to_jump.set_can_transition_to_self(false);
		animator.add_transition(to_jump);
		animator.add_transition(create_transition(Some(2), 3, Some(1.0), 0.125));
		let mut to_rest = create_transition(Some(3), 0, Some(0.5), 0.0);
		to_rest.add_condition("combo", AnimationConditionMode::Equals, 0.0);
		animator.add_transition(to_rest);

		let format_index = |state_index: Option<usize>| state_index.map_or(String::from("-"), |state_index| state_index.to_string());
		let format_frame = |frame: Option<&Rect>| frame.map_or(String::from("-"), |frame| format!("{},{}", frame.position.x, frame.position.y));
		let mut line_list = Vec::new();
		for step in 0..40 {
			if step == 0 {
				animator.set_float("speed", 1.0);
			}
			if step == 6 || step == 8 {
				animator.set_trigger("jump");
			}
			if step == 10 {
				animator.set_integer("combo", 1);
			}
			if step == 14 {
				animator.set_integer("combo", 0);
			}
			if step == 22 {
				animator.set_float("speed", 0.0);
			}
			animator.tick(0.125);
			let trigger_text = if animator.get_trigger("jump") {
				"T"
			}
			else {
				"F"
			};
			line_list.push(format!("{} {} {} {:.4} {:.4} {:.4} {} {} {}", step, format_index(animator.get_current_state_index()), format_index(animator.get_next_state_index()), animator.get_transition_weight(), animator.get_current_normalized_time(), animator.get_next_normalized_time(), format_frame(animator.get_current_frame()), format_frame(animator.get_next_frame()), trigger_text));
		}
		let expected_line_list = [
		"0 0 1 0.0000 0.1250 0.0000 0,0 0,32 F",
		"1 0 1 0.5000 0.2500 0.5000 32,0 64,32 F",
		"2 1 - 0.0000 1.0000 0.0000 0,32 - F",
		"3 1 - 0.0000 1.5000 0.0000 64,32 - F",
		"4 1 - 0.0000 2.0000 0.0000 0,32 - F",
		"5 1 - 0.0000 2.5000 0.0000 64,32 - F",
		"6 2 - 0.0000 0.0000 0.0000 0,64 - F",
		"7 2 - 0.0000 0.2500 0.0000 32,64 - F",
		"8 2 - 0.0000 0.5000 0.0000 64,64 - T",
		"9 2 - 0.0000 0.7500 0.0000 96,64 - T",
		"10 2 - 0.0000 1.0000 0.0000 96,64 - T",
		"11 2 3 0.0000 1.2500 0.0000 96,64 0,96 T",
		"12 2 - 0.0000 0.0000 0.0000 0,64 - F",
		"13 2 - 0.0000 0.2500 0.0000 32,64 - F",
		"14 2 - 0.0000 0.5000 0.0000 64,64 - F",
		"15 2 - 0.0000 0.7500 0.0000 96,64 - F",
		"16 2 - 0.0000 1.0000 0.0000 96,64 - F",
		"17 2 3 0.0000 1.2500 0.0000 96,64 0,96 F",
		"18 3 - 0.0000 0.5000 0.0000 32,96 - F",
		"19 0 - 0.0000 0.0000 0.0000 0,0 - F",
		"20 0 1 0.0000 0.1250 0.0000 0,0 0,32 F",
		"21 0 1 0.5000 0.2500 0.5000 32,0 64,32 F",
		"22 1 0 0.0000 1.0000 0.0000 0,32 0,0 F",
		"23 0 - 0.0000 0.1250 0.0000 0,0 - F",
		"24 0 - 0.0000 0.2500 0.0000 32,0 - F",
		"25 0 - 0.0000 0.3750 0.0000 32,0 - F",
		"26 0 - 0.0000 0.5000 0.0000 64,0 - F",
		"27 0 - 0.0000 0.6250 0.0000 64,0 - F",
		"28 0 - 0.0000 0.7500 0.0000 96,0 - F",
		"29 0 - 0.0000 0.8750 0.0000 96,0 - F",
		"30 0 - 0.0000 1.0000 0.0000 0,0 - F",
		"31 0 - 0.0000 1.1250 0.0000 0,0 - F",
		"32 0 - 0.0000 1.2500 0.0000 32,0 - F",
		"33 0 - 0.0000 1.3750 0.0000 32,0 - F",
		"34 0 - 0.0000 1.5000 0.0000 64,0 - F",
		"35 0 - 0.0000 1.6250 0.0000 64,0 - F",
		"36 0 - 0.0000 1.7500 0.0000 96,0 - F",
		"37 0 - 0.0000 1.8750 0.0000 96,0 - F",
		"38 0 - 0.0000 2.0000 0.0000 0,0 - F",
		"39 0 - 0.0000 2.1250 0.0000 0,0 - F",
		];
		assert_eq!(line_list, expected_line_list);
	}
}
