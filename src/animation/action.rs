//==============================================================================
//! 액션. (차례로 스텝을 실행하는 스크립트 꼴 연출)
//! - 원본 experimental/action.js 를 옮긴 것이다. wait / condition / call / repeat / forever / loop 스텝을 쌓고,
//!   start 뒤 매 프레임 step 을 부르면 차례로 돈다.
//! - 사용:
//!   ```text
//!     let mut action: Action<WorldNode> = Action::new()
//!         .wait(1.0, ActionCallbacks::new().updated(|progress, node: &mut WorldNode| node.set_local_opacity(progress)))
//!         .condition(|node: &WorldNode| node.is_active(), ActionConditionCallbacks::new())
//!         .call(|node: &mut WorldNode| node.set_local_scale(Vector2::create(2.0, 2.0)))
//!         .repeat(3, Action::new().wait(0.5, ActionCallbacks::new()))
//!         .forever(Action::new().wait(2.0, ActionCallbacks::new()));
//!     action.start(&mut node_tree[node_id]);
//!     // 매 프레임:
//!     action.step(&mut node_tree[node_id], time_delta);
//!   ```
//! - 원본과 다른 점:
//!   - 원본은 start(target) 에서 대상을 들고 있다가 step 마다 콜백에 넘긴다. 러스트는 대상을 빌려 들 수 없어서
//!     start 와 step 이 대상(&mut T)을 받아 콜백에 넘긴다. 그래서 get_target 과 start() 의 "대상을 안 주면 이전 대상" 은 없다.
//!     T 는 기본 WorldNode 이고, 노드 여럿이나 게임 상태를 다루려면 그것을 T 로 쓴다.
//!   - 콜백 객체({ started, updated, completed })는 wait 스텝이 ActionCallbacks(updated 가 (진행률, 대상)),
//!     condition 스텝이 ActionConditionCallbacks(updated 가 (대상))다. 원본은 한 객체 꼴에 인자 수만 다르지만,
//!     러스트는 콜백의 인자 꼴을 타입으로 정해야 해서 둘로 나눴다.
//!   - loop 는 러스트 예약어라 r#loop 로 부른다. (Tween 의 r#in 과 같다)
//!   - call 스텝이 이어지면 원본은 재귀로, 러스트 판은 반복문으로 넘긴다. (결과는 같다)
//! - 조건 함수는 원본처럼 대상을 받는다(러스트는 &T). wait 은 duration 이 0 이하이면 진행률 1 로 끝낸다.
//!   repeat 은 횟수가 0 이면 돌지 않고 넘어간다. (원본과 같다)
//==============================================================================
use crate::core::node::worldnode::WorldNode;

/// 시작 / 완료 콜백. ((대상))
type ActionTargetCallback<T> = Box<dyn FnMut(&mut T)>;
/// 갱신 콜백. ((진행률, 대상))
type ActionUpdateCallback<T> = Box<dyn FnMut(f32, &mut T)>;
/// 조건 함수. ((대상) → 참이면 넘어간다)
type ActionConditionFunction<T> = Box<dyn FnMut(&T) -> bool>;

//==============================================================================
/// 스텝 콜백 묶음. (원본 { started, updated, completed })
/// - wait 스텝에 넘긴다. 빌더처럼 started, updated, completed 를 이어 부른다. 비워 둔 콜백은 부르지 않는다.
///
/// # 예제
/// wait 스텝의 시작, 진행률, 완료 콜백
/// ```rust
/// use vanilla::{Action, ActionCallbacks};
///
/// let mut record: Vec<f32> = Vec::new();
/// let mut action: Action<Vec<f32>> = Action::new()
/// 	.wait(1.0, ActionCallbacks::new()
/// 		.started(|record: &mut Vec<f32>| record.push(-1.0))
/// 		.updated(|progress, record: &mut Vec<f32>| record.push(progress))
/// 		.completed(|record: &mut Vec<f32>| record.push(2.0)));
/// action.start(&mut record);
/// action.step(&mut record, 0.5);
/// action.step(&mut record, 0.5);
/// assert_eq!(record, vec![-1.0, 0.5, 1.0, 2.0]);
/// assert!(action.is_done());
/// ```
//==============================================================================
pub struct ActionCallbacks<T> {
	started_callback: Option<ActionTargetCallback<T>>,
	updated_callback: Option<ActionUpdateCallback<T>>,
	completed_callback: Option<ActionTargetCallback<T>>,
}

impl<T> ActionCallbacks<T> {
	//==============================================================================
	/// 생성. (모두 비어 있음, 원본 {})
	///
	/// # 돌려줌
	/// 콜백이 하나도 없는 묶음.
	//==============================================================================
	pub fn new() -> ActionCallbacks<T> {
		ActionCallbacks {
			started_callback: None,
			updated_callback: None,
			completed_callback: None,
		}
	}

	//==============================================================================
	/// 스텝 시작 때 부를 콜백. ((대상))
	///
	/// # 인자
	/// - `callback`: 스텝이 시작될 때 한 번 부를 함수. (대상을 받는다) 이미 있던 시작 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 시작 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn started(mut self, callback: impl FnMut(&mut T) + 'static) -> ActionCallbacks<T> {
		self.started_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 매 프레임 부를 콜백. ((진행률 0 ~ 1, 대상). condition 스텝은 진행률 0)
	///
	/// # 인자
	/// - `callback`: 스텝이 도는 동안 step 마다 부를 함수. (진행률 0 ~ 1 과 대상을 받는다, 마지막 프레임은 1) 이미 있던 갱신 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 갱신 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn updated(mut self, callback: impl FnMut(f32, &mut T) + 'static) -> ActionCallbacks<T> {
		self.updated_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 스텝 완료 때 부를 콜백. ((대상))
	///
	/// # 인자
	/// - `callback`: 스텝이 끝날 때 한 번 부를 함수. (마지막 갱신 콜백 바로 뒤, 대상을 받는다) 이미 있던 완료 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 완료 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn completed(mut self, callback: impl FnMut(&mut T) + 'static) -> ActionCallbacks<T> {
		self.completed_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 시작 콜백 부르기.
	//==============================================================================
	fn call_started(&mut self, target: &mut T) {
		if let Some(started_callback) = self.started_callback.as_mut() {
			started_callback(target);
		}
	}

	//==============================================================================
	/// 갱신 콜백 부르기.
	//==============================================================================
	fn call_updated(&mut self, progress: f32, target: &mut T) {
		if let Some(updated_callback) = self.updated_callback.as_mut() {
			updated_callback(progress, target);
		}
	}

	//==============================================================================
	/// 완료 콜백 부르기.
	//==============================================================================
	fn call_completed(&mut self, target: &mut T) {
		if let Some(completed_callback) = self.completed_callback.as_mut() {
			completed_callback(target);
		}
	}
}

impl<T> Default for ActionCallbacks<T> {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ActionCallbacks<T> {
		ActionCallbacks::new()
	}
}

//==============================================================================
/// condition 스텝 콜백 묶음. (원본 { started, updated, completed }, updated 가 (대상)만 받는다)
/// - condition 스텝에 넘긴다. 빌더처럼 이어 부르고, 비워 둔 콜백은 부르지 않는다.
///
/// # 예제
/// 문이 열릴 때까지 기다린 프레임 세기
/// ```rust
/// use vanilla::{Action, ActionConditionCallbacks};
///
/// struct Door {
/// 	is_open: bool,
/// 	waited_frame_count: u32,
/// 	is_passed: bool,
/// }
///
/// let mut door = Door { is_open: false, waited_frame_count: 0, is_passed: false };
/// let mut action: Action<Door> = Action::new()
/// 	.condition(|door: &Door| door.is_open, ActionConditionCallbacks::new()
/// 		.updated(|door: &mut Door| door.waited_frame_count += 1)
/// 		.completed(|door: &mut Door| door.is_passed = true));
/// action.start(&mut door);
/// action.step(&mut door, 0.016);
/// action.step(&mut door, 0.016);
/// assert!(!door.is_passed);
/// door.is_open = true;
/// action.step(&mut door, 0.016);
/// assert_eq!(door.waited_frame_count, 3);
/// assert!(door.is_passed);
/// assert!(action.is_done());
/// ```
//==============================================================================
pub struct ActionConditionCallbacks<T> {
	started_callback: Option<ActionTargetCallback<T>>,
	updated_callback: Option<ActionTargetCallback<T>>,
	completed_callback: Option<ActionTargetCallback<T>>,
}

impl<T> ActionConditionCallbacks<T> {
	//==============================================================================
	/// 생성. (모두 비어 있음, 원본 {})
	///
	/// # 돌려줌
	/// 콜백이 하나도 없는 묶음.
	//==============================================================================
	pub fn new() -> ActionConditionCallbacks<T> {
		ActionConditionCallbacks {
			started_callback: None,
			updated_callback: None,
			completed_callback: None,
		}
	}

	//==============================================================================
	/// 스텝 시작 때 부를 콜백. ((대상))
	///
	/// # 인자
	/// - `callback`: 스텝이 시작될 때 한 번 부를 함수. (대상을 받는다) 이미 있던 시작 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 시작 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn started(mut self, callback: impl FnMut(&mut T) + 'static) -> ActionConditionCallbacks<T> {
		self.started_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 조건 충족 전 매 프레임 부를 콜백. ((대상))
	///
	/// # 인자
	/// - `callback`: step 마다 조건을 묻기 전에 부를 함수. (대상을 받는다, 조건이 참이 되는 프레임에도 한 번 부른다) 이미 있던 갱신 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 갱신 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn updated(mut self, callback: impl FnMut(&mut T) + 'static) -> ActionConditionCallbacks<T> {
		self.updated_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 조건 충족 때 부를 콜백. ((대상))
	///
	/// # 인자
	/// - `callback`: 조건이 참이 된 프레임에 한 번 부를 함수. (대상을 받는다) 이미 있던 완료 콜백은 바뀐다.
	///
	/// # 돌려줌
	/// 완료 콜백을 넣은 묶음. (이어 부르기용)
	//==============================================================================
	pub fn completed(mut self, callback: impl FnMut(&mut T) + 'static) -> ActionConditionCallbacks<T> {
		self.completed_callback = Some(Box::new(callback));
		self
	}

	//==============================================================================
	/// 시작 콜백 부르기.
	//==============================================================================
	fn call_started(&mut self, target: &mut T) {
		if let Some(started_callback) = self.started_callback.as_mut() {
			started_callback(target);
		}
	}

	//==============================================================================
	/// 갱신 콜백 부르기.
	//==============================================================================
	fn call_updated(&mut self, target: &mut T) {
		if let Some(updated_callback) = self.updated_callback.as_mut() {
			updated_callback(target);
		}
	}

	//==============================================================================
	/// 완료 콜백 부르기.
	//==============================================================================
	fn call_completed(&mut self, target: &mut T) {
		if let Some(completed_callback) = self.completed_callback.as_mut() {
			completed_callback(target);
		}
	}
}

impl<T> Default for ActionConditionCallbacks<T> {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ActionConditionCallbacks<T> {
		ActionConditionCallbacks::new()
	}
}

//==============================================================================
/// 스텝 하나.
//==============================================================================
enum ActionStep<T> {
	/// 일정 시간 대기.
	Wait {
		duration: f32,
		callbacks: ActionCallbacks<T>,
	},
	/// 조건이 참이 될 때까지 대기.
	Condition {
		condition_function: ActionConditionFunction<T>,
		callbacks: ActionConditionCallbacks<T>,
	},
	/// 즉시 호출.
	Call {
		function: ActionTargetCallback<T>,
	},
	/// 안쪽 액션을 times 번 반복.
	Repeat {
		times: u32,
		inner_action: Box<Action<T>>,
	},
	/// 안쪽 액션을 끝없이 반복.
	Forever {
		inner_action: Box<Action<T>>,
	},
	/// 조건이 참인 동안 안쪽 액션을 반복. (원본 loop)
	WhileLoop {
		condition_function: ActionConditionFunction<T>,
		inner_action: Box<Action<T>>,
	},
}

//==============================================================================
/// 액션.
/// - 스텝을 빌더처럼 쌓고 start 뒤 매 프레임 step 을 부르면 차례로 돈다. 대상 꼴 T 는 기본 WorldNode 이다.
///
/// # 예제
/// 노드를 서서히 보이고, 활성인지 보고, 키운 뒤 반복 대기
/// ```rust
/// use vanilla::{Action, ActionCallbacks, ActionConditionCallbacks, Vector2, WorldNode};
///
/// let mut node = WorldNode::new("hero");
/// let mut action: Action<WorldNode> = Action::new()
/// 	.wait(1.0, ActionCallbacks::new().updated(|progress, node: &mut WorldNode| node.set_local_opacity(progress)))
/// 	.condition(|node: &WorldNode| node.is_active(), ActionConditionCallbacks::new())
/// 	.call(|node: &mut WorldNode| node.set_local_scale(Vector2::create(2.0, 2.0)))
/// 	.repeat(3, Action::new().wait(0.5, ActionCallbacks::new()))
/// 	.forever(Action::new().wait(2.0, ActionCallbacks::new()));
/// action.start(&mut node);
/// action.step(&mut node, 0.5);
/// assert_eq!(node.get_local_opacity(), 0.5);
/// action.step(&mut node, 0.5);
/// assert_eq!(node.get_local_opacity(), 1.0);
/// action.step(&mut node, 0.0);
/// assert_eq!(node.get_local_scale(), Vector2::create(2.0, 2.0));
/// for _ in 0..10 {
/// 	action.step(&mut node, 0.5);
/// }
/// assert!(action.is_running());
/// assert!(!action.is_done());
/// ```
//==============================================================================
pub struct Action<T = WorldNode> {
	steps: Vec<ActionStep<T>>,
	current_step_index: usize,
	current_step_elapsed: f32,
	// 지금 repeat 스텝이 끝낸 횟수. (원본 currentStepState.completed_count)
	completed_count: u32,
	is_running: bool,
	is_done: bool,
}

impl<T> Action<T> {
	//==============================================================================
	/// 생성. (스텝 없음, 멈춤)
	///
	/// # 돌려줌
	/// 스텝이 없고 돌지 않는 액션. (스텝 없이 start 하면 바로 끝난다)
	//==============================================================================
	pub fn new() -> Action<T> {
		Action {
			steps: Vec::new(),
			current_step_index: 0,
			current_step_elapsed: 0.0,
			completed_count: 0,
			is_running: false,
			is_done: false,
		}
	}

	//==============================================================================
	/// 현재 스텝 시작 처리. (call 스텝과 조건이 거짓인 loop 스텝은 그 자리에서 넘긴다)
	//==============================================================================
	fn start_current_step(&mut self, target: &mut T) {
		loop {
			if self.current_step_index >= self.steps.len() {
				self.is_running = false;
				self.is_done = true;
				return;
			}
			self.current_step_elapsed = 0.0;
			self.completed_count = 0;
			match &mut self.steps[self.current_step_index] {
				ActionStep::Call { function } => {
					function(target);
					self.current_step_index += 1;
				}
				ActionStep::Wait { callbacks, .. } => {
					callbacks.call_started(target);
					return;
				}
				ActionStep::Condition { callbacks, .. } => {
					callbacks.call_started(target);
					return;
				}
				ActionStep::Repeat { times, inner_action } => {
					if *times == 0 {
						self.current_step_index += 1;
						continue;
					}
					inner_action.start(target);
					return;
				}
				ActionStep::Forever { inner_action } => {
					inner_action.start(target);
					return;
				}
				ActionStep::WhileLoop { condition_function, inner_action } => {
					if !condition_function(target) {
						self.current_step_index += 1;
						continue;
					}
					inner_action.start(target);
					return;
				}
			}
		}
	}

	//==============================================================================
	/// 다음 스텝으로 이동.
	//==============================================================================
	fn advance_step(&mut self, target: &mut T) {
		self.current_step_index += 1;
		self.start_current_step(target);
	}

	//==============================================================================
	/// 일정 시간 대기. (started(대상), updated(진행률 0 ~ 1, 대상) 매 프레임, completed(대상))
	///
	/// # 인자
	/// - `duration`: 기다릴 시간. (초, step 에 넘긴 time_delta 를 더해 잰다) 0 이하이면 첫 step 에서 진행률 1 로 끝난다.
	/// - `callbacks`: 시작, 갱신, 완료 콜백 묶음.
	///
	/// # 돌려줌
	/// wait 스텝을 끝에 더한 액션. (이어 부르기용)
	//==============================================================================
	pub fn wait(mut self, duration: f32, callbacks: ActionCallbacks<T>) -> Action<T> {
		self.steps.push(ActionStep::Wait { duration, callbacks });
		self
	}

	//==============================================================================
	/// 조건이 참이 될 때까지 대기. (started(대상), updated(대상) 조건 충족 전 매 프레임, completed(대상))
	///
	/// # 인자
	/// - `condition_function`: 대상을 받아 참이면 넘어가는 함수. (start 에서는 묻지 않고 step 마다 묻는다)
	/// - `callbacks`: 시작, 갱신, 완료 콜백 묶음.
	///
	/// # 돌려줌
	/// condition 스텝을 끝에 더한 액션. (이어 부르기용)
	//==============================================================================
	pub fn condition(mut self, condition_function: impl FnMut(&T) -> bool + 'static, callbacks: ActionConditionCallbacks<T>) -> Action<T> {
		self.steps.push(ActionStep::Condition { condition_function: Box::new(condition_function), callbacks });
		self
	}

	//==============================================================================
	/// 즉시 콜백 호출.
	///
	/// # 인자
	/// - `function`: 이 스텝에 닿자마자 부를 함수. (대상을 받는다, 같은 프레임에 다음 스텝으로 넘어간다)
	///
	/// # 돌려줌
	/// call 스텝을 끝에 더한 액션. (이어 부르기용)
	//==============================================================================
	pub fn call(mut self, function: impl FnMut(&mut T) + 'static) -> Action<T> {
		self.steps.push(ActionStep::Call { function: Box::new(function) });
		self
	}

	//==============================================================================
	/// 안쪽 액션을 times 번 반복. (0 이면 돌지 않고 넘어간다)
	///
	/// # 인자
	/// - `times`: 반복 횟수. 0 이면 안쪽을 시작하지 않고 넘어간다.
	/// - `inner_action`: 반복할 액션. (끝날 때마다 처음부터 다시 start 한다)
	///
	/// # 돌려줌
	/// repeat 스텝을 끝에 더한 액션. (이어 부르기용)
	///
	/// # 예제
	/// 0.5초 대기를 세 번
	/// ```rust
	/// use vanilla::{Action, ActionCallbacks};
	///
	/// let mut count: u32 = 0;
	/// let inner_action: Action<u32> = Action::new().wait(0.5, ActionCallbacks::new().completed(|count: &mut u32| *count += 1));
	/// let mut action: Action<u32> = Action::new().repeat(3, inner_action);
	/// action.start(&mut count);
	/// for _ in 0..3 {
	/// 	action.step(&mut count, 0.5);
	/// }
	/// assert_eq!(count, 3);
	/// assert!(action.is_done());
	/// ```
	//==============================================================================
	pub fn repeat(mut self, times: u32, inner_action: Action<T>) -> Action<T> {
		self.steps.push(ActionStep::Repeat { times, inner_action: Box::new(inner_action) });
		self
	}

	//==============================================================================
	/// 안쪽 액션을 끝없이 반복.
	///
	/// # 인자
	/// - `inner_action`: 반복할 액션. (끝날 때마다 처음부터 다시 start 한다)
	///
	/// # 돌려줌
	/// forever 스텝을 끝에 더한 액션. (이어 부르기용, 이 뒤의 스텝에는 닿지 않는다. 멈추려면 stop)
	//==============================================================================
	pub fn forever(mut self, inner_action: Action<T>) -> Action<T> {
		self.steps.push(ActionStep::Forever { inner_action: Box::new(inner_action) });
		self
	}

	//==============================================================================
	/// 조건이 참인 동안 안쪽 액션을 반복. (시작할 때와 안쪽이 끝날 때마다 묻는다)
	/// - loop 는 러스트 예약어라 r#loop 로 부른다.
	///
	/// # 인자
	/// - `condition_function`: 대상을 받아 참이면 안쪽을 (다시) 돌리는 함수. 처음부터 거짓이면 안쪽을 건너뛴다.
	/// - `inner_action`: 반복할 액션.
	///
	/// # 돌려줌
	/// loop 스텝을 끝에 더한 액션. (이어 부르기용)
	///
	/// # 예제
	/// 값이 3 이 될 때까지 0.1초마다 하나씩 올리기
	/// ```rust
	/// use vanilla::{Action, ActionCallbacks};
	///
	/// let mut count: u32 = 0;
	/// let inner_action: Action<u32> = Action::new().wait(0.1, ActionCallbacks::new().completed(|count: &mut u32| *count += 1));
	/// let mut action: Action<u32> = Action::new().r#loop(|count: &u32| *count < 3, inner_action);
	/// action.start(&mut count);
	/// for _ in 0..10 {
	/// 	action.step(&mut count, 0.1);
	/// }
	/// assert_eq!(count, 3);
	/// assert!(action.is_done());
	/// ```
	//==============================================================================
	pub fn r#loop(mut self, condition_function: impl FnMut(&T) -> bool + 'static, inner_action: Action<T>) -> Action<T> {
		self.steps.push(ActionStep::WhileLoop { condition_function: Box::new(condition_function), inner_action: Box::new(inner_action) });
		self
	}

	//==============================================================================
	/// 실행 시작. (처음 스텝부터)
	/// - 앞에 있는 call 스텝은 이 자리에서 바로 돈다. 돌던 중이면 처음부터 다시 한다.
	///
	/// # 인자
	/// - `target`: 콜백에 넘길 대상. (첫 스텝의 시작 콜백까지 이 자리에서 부른다)
	//==============================================================================
	pub fn start(&mut self, target: &mut T) {
		self.current_step_index = 0;
		self.current_step_elapsed = 0.0;
		self.completed_count = 0;
		self.is_running = true;
		self.is_done = false;
		self.start_current_step(target);
	}

	//==============================================================================
	/// 실행 중단. (끝난 것으로 친다)
	//==============================================================================
	pub fn stop(&mut self) {
		self.is_running = false;
		self.is_done = true;
	}

	//==============================================================================
	/// 갱신. (매 프레임 호출)
	/// - start 전이거나 끝났으면 아무것도 하지 않는다. 한 번에 스텝 하나만 진행하고, 끝난 스텝 뒤의 call 스텝은 같은 프레임에 돈다.
	///
	/// # 인자
	/// - `target`: 콜백에 넘길 대상.
	/// - `time_delta`: 지난 프레임에서 흐른 시간. (초, wait 스텝이 이것을 더해 잰다)
	///
	/// # 예제
	/// 씬 tick 에서 액션 돌리기
	/// ```rust
	/// use vanilla::{Action, ActionCallbacks, WorldNode};
	///
	/// struct FadeIn {
	/// 	node: WorldNode,
	/// 	action: Action,
	/// }
	///
	/// impl FadeIn {
	/// 	fn new() -> FadeIn {
	/// 		let mut fade_in = FadeIn {
	/// 			node: WorldNode::new("logo"),
	/// 			action: Action::new().wait(2.0, ActionCallbacks::new().updated(|progress, node: &mut WorldNode| node.set_local_opacity(progress))),
	/// 		};
	/// 		fade_in.action.start(&mut fade_in.node);
	/// 		fade_in
	/// 	}
	///
	/// 	fn tick(&mut self, time_delta: f32) {
	/// 		self.action.step(&mut self.node, time_delta);
	/// 	}
	/// }
	///
	/// let mut fade_in = FadeIn::new();
	/// fade_in.tick(1.0);
	/// assert_eq!(fade_in.node.get_local_opacity(), 0.5);
	/// fade_in.tick(1.0);
	/// assert!(fade_in.action.is_done());
	/// ```
	//==============================================================================
	pub fn step(&mut self, target: &mut T, time_delta: f32) {
		if !self.is_running || self.is_done {
			return;
		}
		if self.current_step_index >= self.steps.len() {
			self.is_running = false;
			self.is_done = true;
			return;
		}
		let mut is_advancing = false;
		match &mut self.steps[self.current_step_index] {
			ActionStep::Wait { duration, callbacks } => {
				self.current_step_elapsed += time_delta;
				let progress = if *duration > 0.0 {
					(self.current_step_elapsed / *duration).clamp(0.0, 1.0)
				}
				else {
					1.0
				};
				callbacks.call_updated(progress, target);
				if self.current_step_elapsed >= *duration {
					callbacks.call_completed(target);
					is_advancing = true;
				}
			}
			ActionStep::Condition { condition_function, callbacks } => {
				callbacks.call_updated(target);
				if condition_function(target) {
					callbacks.call_completed(target);
					is_advancing = true;
				}
			}
			ActionStep::Repeat { times, inner_action } => {
				inner_action.step(target, time_delta);
				if inner_action.is_done() {
					self.completed_count += 1;
					if self.completed_count >= *times {
						is_advancing = true;
					}
					else {
						inner_action.start(target);
					}
				}
			}
			ActionStep::Forever { inner_action } => {
				inner_action.step(target, time_delta);
				if inner_action.is_done() {
					inner_action.start(target);
				}
			}
			ActionStep::WhileLoop { condition_function, inner_action } => {
				inner_action.step(target, time_delta);
				if inner_action.is_done() {
					if condition_function(target) {
						inner_action.start(target);
					}
					else {
						is_advancing = true;
					}
				}
			}
			ActionStep::Call { .. } => {
				// call 은 시작할 때 이미 넘겼다.
			}
		}
		if is_advancing {
			self.advance_step(target);
		}
	}

	//==============================================================================
	/// 완료 여부 반환.
	///
	/// # 돌려줌
	/// 마지막 스텝까지 끝났거나 stop 했으면 true. (start 전에는 false, forever 는 stop 전까지 false)
	//==============================================================================
	pub fn is_done(&self) -> bool {
		self.is_done
	}

	//==============================================================================
	/// 실행 중 여부 반환.
	///
	/// # 돌려줌
	/// start 뒤 아직 끝나지 않았으면 true.
	//==============================================================================
	pub fn is_running(&self) -> bool {
		self.is_running
	}
}

impl<T> Default for Action<T> {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Action<T> {
		Action::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::vector2::Vector2;

	//==============================================================================
	/// 시험용 대상. (적은 기록과 조건 값)
	//==============================================================================
	#[derive(Default)]
	struct Recorder {
		log: Vec<String>,
		is_ready: bool,
		counter: u32,
	}

	//==============================================================================
	/// wait 은 시작, 매 프레임 진행률, 완료를 차례로 부르고 다음 스텝으로 넘어간다.
	//==============================================================================
	#[test]
	fn wait_calls_callbacks_in_order() {
		let mut recorder = Recorder::default();
		let mut action: Action<Recorder> = Action::new()
			.wait(1.0, ActionCallbacks::new()
				.started(|recorder: &mut Recorder| recorder.log.push(String::from("started")))
				.updated(|progress, recorder: &mut Recorder| recorder.log.push(format!("updated {:.2}", progress)))
				.completed(|recorder: &mut Recorder| recorder.log.push(String::from("completed"))))
			.call(|recorder: &mut Recorder| recorder.log.push(String::from("called")));
		action.start(&mut recorder);
		assert_eq!(recorder.log, vec!["started"]);
		action.step(&mut recorder, 0.5);
		action.step(&mut recorder, 0.75);
		assert_eq!(recorder.log, vec!["started", "updated 0.50", "updated 1.00", "completed", "called"]);
		assert!(action.is_done());
		assert!(!action.is_running());
		action.step(&mut recorder, 1.0);
		assert_eq!(recorder.log.len(), 5);
	}

	//==============================================================================
	/// condition 은 조건이 참이 될 때까지 매 프레임 묻고, call 이 앞에 있으면 start 에서 바로 돈다.
	//==============================================================================
	#[test]
	fn condition_waits_until_true() {
		let mut recorder = Recorder::default();
		let mut action: Action<Recorder> = Action::new()
			.call(|recorder: &mut Recorder| recorder.counter += 1)
			.condition(|recorder: &Recorder| recorder.is_ready, ActionConditionCallbacks::new().updated(|recorder: &mut Recorder| recorder.log.push(String::from("waiting"))))
			.call(|recorder: &mut Recorder| recorder.counter += 10);
		action.start(&mut recorder);
		assert_eq!(recorder.counter, 1);
		action.step(&mut recorder, 0.1);
		action.step(&mut recorder, 0.1);
		assert_eq!(recorder.log, vec!["waiting", "waiting"]);
		assert!(!action.is_done());
		recorder.is_ready = true;
		action.step(&mut recorder, 0.1);
		assert_eq!(recorder.counter, 11);
		assert!(action.is_done());
	}

	//==============================================================================
	/// repeat 은 안쪽 액션을 정한 횟수만큼 돌리고 넘어간다. (0 이면 돌지 않는다)
	//==============================================================================
	#[test]
	fn repeat_runs_inner_action_times() {
		let mut recorder = Recorder::default();
		let inner_action: Action<Recorder> = Action::new().wait(0.5, ActionCallbacks::new().completed(|recorder: &mut Recorder| recorder.counter += 1));
		let skipped_action: Action<Recorder> = Action::new().wait(0.5, ActionCallbacks::new().completed(|recorder: &mut Recorder| recorder.counter += 100));
		let mut action: Action<Recorder> = Action::new().repeat(3, inner_action).repeat(0, skipped_action);
		action.start(&mut recorder);
		for _ in 0..2 {
			action.step(&mut recorder, 0.5);
		}
		assert_eq!(recorder.counter, 2);
		assert!(!action.is_done());
		action.step(&mut recorder, 0.5);
		assert_eq!(recorder.counter, 3);
		assert!(action.is_done());
	}

	//==============================================================================
	/// loop 는 조건이 참인 동안 안쪽을 다시 돌리고, 처음부터 거짓이면 건너뛴다. forever 는 끝나지 않는다. stop 은 끝낸다.
	//==============================================================================
	#[test]
	fn loop_forever_and_stop() {
		let mut recorder = Recorder::default();
		let mut loop_action: Action<Recorder> = Action::new()
			.r#loop(|recorder: &Recorder| recorder.counter < 2, Action::new().wait(0.1, ActionCallbacks::new().completed(|recorder: &mut Recorder| recorder.counter += 1)))
			.r#loop(|_recorder: &Recorder| false, Action::new().call(|recorder: &mut Recorder| recorder.counter += 1000))
			.call(|recorder: &mut Recorder| recorder.log.push(String::from("after loop")));
		loop_action.start(&mut recorder);
		loop_action.step(&mut recorder, 0.1);
		assert!(!loop_action.is_done());
		loop_action.step(&mut recorder, 0.1);
		assert_eq!(recorder.counter, 2);
		assert_eq!(recorder.log, vec!["after loop"]);
		assert!(loop_action.is_done());

		let mut forever_action: Action<Recorder> = Action::new().forever(Action::new().wait(0.1, ActionCallbacks::new().completed(|recorder: &mut Recorder| recorder.counter += 1)));
		forever_action.start(&mut recorder);
		for _ in 0..5 {
			forever_action.step(&mut recorder, 0.1);
		}
		assert_eq!(recorder.counter, 7);
		assert!(forever_action.is_running());
		forever_action.stop();
		forever_action.step(&mut recorder, 0.1);
		assert_eq!(recorder.counter, 7);
		assert!(forever_action.is_done());
	}

	//==============================================================================
	/// 기본 대상은 WorldNode 다. (노드 투명도를 진행률로 올린다)
	//==============================================================================
	#[test]
	fn world_node_target_fades_in() {
		let mut node = WorldNode::new("fade");
		let mut action: Action = Action::new()
			.call(|node: &mut WorldNode| node.set_local_opacity(0.0))
			.wait(2.0, ActionCallbacks::new().updated(|progress, node: &mut WorldNode| node.set_local_opacity(progress)))
			.call(|node: &mut WorldNode| node.set_local_scale(Vector2::create(2.0, 2.0)));
		action.start(&mut node);
		assert_eq!(node.get_local_opacity(), 0.0);
		action.step(&mut node, 1.0);
		assert_eq!(node.get_local_opacity(), 0.5);
		action.step(&mut node, 1.0);
		assert_eq!(node.get_local_opacity(), 1.0);
		assert_eq!(node.get_local_scale(), Vector2::create(2.0, 2.0));
		assert!(action.is_done());
	}
}
