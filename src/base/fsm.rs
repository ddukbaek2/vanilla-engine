//==============================================================================
//! 유한 상태 기계. (원본 base/fsm.js 의 FiniteStateMachine)
//! - 여러 게임이 "문자열 상태 + switch 문 + 수동 전이" 를 제각각 만들던 것을 표준화했다.
//! - 상태마다 enter / tick / exit 처리를 등록하고 change_state() 로 전이한다.
//! - set_transition_guard() 로 허용되지 않은 전이를 막을 수 있다.
//! - 원본 상태 이름은 문자열이다. 러스트 판은 상태 이름 타입을 고를 수 있게 했다. (기본은 String, 열거형이나 &'static str 도 된다)
//!   원본의 null 상태(아직 아무 상태도 아님)는 None 이다.
//! - 원본 handlers 객체 { enter?, tick?, exit? } 는 StateHandlers 구조체다. 콜백은 `Box<dyn FnMut>` 라서
//!   게임 상태를 바꾸려면 `Rc<Cell>` / `Rc<RefCell>` 을 잡아 넣는다.
//! - 원본은 처리 함수 안에서 같은 기계의 change_state 를 그 자리에서 부른다. 러스트 판은 처리 함수가 기계를 빌릴 수 없어서
//!   get_state_change_requester 로 받은 손잡이(StateChangeRequester)를 처리 함수에 잡아 넣고 그것의 change_state 로 부탁한다.
//!   기계는 그 처리 함수가 끝난 뒤 부탁받은 차례대로 전이한다. (원본에서 처리 함수가 change_state 를 맨 끝에 부른 것과 같다)
//!   부탁의 성공 여부는 그 자리에서 돌려주지 않는다. 처리 함수 밖에서 부탁하면 다음 tick 이나 change_state 때 전이한다.
//==============================================================================
use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

/// 상태에 들어갈 때 부른다. (이전 상태 이름, 처음이면 None)
pub type StateEnterHandler<StateName> = Box<dyn FnMut(Option<&StateName>)>;
/// 상태에 머무는 동안 tick 마다 부른다. (time_delta, 이 상태에 머문 초)
pub type StateTickHandler = Box<dyn FnMut(f64, f64)>;
/// 상태에서 나갈 때 부른다. (다음 상태 이름)
pub type StateExitHandler<StateName> = Box<dyn FnMut(&StateName)>;
/// 전이 가드. (이전 상태 이름, 다음 상태 이름) → 허용 여부
pub type TransitionGuard<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName) -> bool>;
/// 전이 알림. (이전 상태 이름, 다음 상태 이름) exit 뒤, enter 앞에 불린다.
pub type TransitionEvent<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName)>;

//==============================================================================
/// 상태 처리 함수 묶음. (원본 addState 의 handlers: { enter?, tick?, exit? })
///
/// # 예제
/// tick 처리 함수만 두고 나머지는 기본값
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{FiniteStateMachine, StateHandlers};
///
/// let total_seconds = Rc::new(Cell::new(0.0f64));
/// let total_seconds_in_tick = total_seconds.clone();
/// let mut state_machine: FiniteStateMachine<&'static str> = FiniteStateMachine::new();
/// state_machine.add_state("play", StateHandlers {
/// 	tick: Some(Box::new(move |time_delta: f64, _state_seconds: f64| {
/// 		total_seconds_in_tick.set(total_seconds_in_tick.get() + time_delta);
/// 	})),
/// 	..StateHandlers::default()
/// });
/// state_machine.change_state("play", false);
/// state_machine.tick(0.25);
/// state_machine.tick(0.25);
/// assert_eq!(total_seconds.get(), 0.5);
/// ```
//==============================================================================
pub struct StateHandlers<StateName> {
	/// 상태에 들어갈 때 부르는 함수. (이전 상태 이름을 받는다, 처음이면 None) None 이면 부르지 않는다.
	pub enter: Option<StateEnterHandler<StateName>>,
	/// 상태에 머무는 동안 tick 마다 부르는 함수. (time_delta 초, 이 상태에 머문 초) None 이면 부르지 않는다.
	pub tick: Option<StateTickHandler>,
	/// 상태에서 나갈 때 부르는 함수. (다음 상태 이름을 받는다) None 이면 부르지 않는다.
	pub exit: Option<StateExitHandler<StateName>>,
}

impl<StateName> Default for StateHandlers<StateName> {
	//==============================================================================
	/// 처리 함수가 하나도 없는 묶음. (원본 기본 인자 handlers = {})
	//==============================================================================
	fn default() -> StateHandlers<StateName> {
		StateHandlers { enter: None, tick: None, exit: None }
	}
}

//==============================================================================
/// 처리 함수 안에서 전이를 부탁하는 손잡이. (FiniteStateMachine::get_state_change_requester 가 주고, 복제해 처리 함수에 잡아 넣는다)
///
/// # 예제
/// 1 초 머문 뒤 tick 처리 함수에서 다음 상태 부탁하기
/// ```rust
/// use vanilla::{FiniteStateMachine, StateHandlers};
///
/// let mut state_machine: FiniteStateMachine<&'static str> = FiniteStateMachine::new();
/// let state_change_requester = state_machine.get_state_change_requester();
/// state_machine.add_state("countdown", StateHandlers {
/// 	tick: Some(Box::new(move |_time_delta: f64, state_seconds: f64| {
/// 		if state_seconds >= 1.0 {
/// 			state_change_requester.change_state("play", false);
/// 		}
/// 	})),
/// 	..StateHandlers::default()
/// });
/// state_machine.add_state("play", StateHandlers::default());
/// state_machine.change_state("countdown", false);
/// state_machine.tick(0.5);
/// assert_eq!(state_machine.get_state_name(), Some(&"countdown"));
/// state_machine.tick(0.5);
/// assert_eq!(state_machine.get_state_name(), Some(&"play"));
/// ```
//==============================================================================
pub struct StateChangeRequester<StateName> {
	/// 부탁받은 전이. (다음 상태 이름, is_forced)
	pending_change_list: Rc<RefCell<Vec<(StateName, bool)>>>,
}

impl<StateName> Clone for StateChangeRequester<StateName> {
	//==============================================================================
	/// 같은 기계를 가리키는 손잡이 복제.
	//==============================================================================
	fn clone(&self) -> StateChangeRequester<StateName> {
		StateChangeRequester {
			pending_change_list: self.pending_change_list.clone(),
		}
	}
}

impl<StateName> StateChangeRequester<StateName> {
	//==============================================================================
	/// 상태 전이 부탁. (원본 처리 함수 안의 change_state, 기계가 지금 처리 함수가 끝난 뒤 차례대로 전이한다)
	///
	/// # 인자
	/// - `next_state_name`: 옮겨 갈 상태 이름. (등록하지 않은 이름이면 기계가 전이하지 않는다)
	/// - `is_forced`: 참이면 같은 상태로도 다시 전이한다. (전이 가드는 그대로 본다)
	//==============================================================================
	pub fn change_state(&self, next_state_name: StateName, is_forced: bool) {
		self.pending_change_list.borrow_mut().push((next_state_name, is_forced));
	}
}

//==============================================================================
/// 유한 상태 기계.
///
/// # 예제
/// 열거형 상태를 등록하고 갱신하며 전이하기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{FiniteStateMachine, StateHandlers};
///
/// #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
/// enum PlayerState {
/// 	Idle,
/// 	Run,
/// }
///
/// let record_list = Rc::new(RefCell::new(Vec::<String>::new()));
/// let enter_record_list = record_list.clone();
/// let exit_record_list = record_list.clone();
/// let mut state_machine = FiniteStateMachine::<PlayerState>::new();
/// state_machine.add_state(PlayerState::Idle, StateHandlers {
/// 	exit: Some(Box::new(move |next_state_name: &PlayerState| {
/// 		exit_record_list.borrow_mut().push(format!("idle exit to {:?}", next_state_name));
/// 	})),
/// 	..StateHandlers::default()
/// });
/// state_machine.add_state(PlayerState::Run, StateHandlers {
/// 	enter: Some(Box::new(move |previous_state_name: Option<&PlayerState>| {
/// 		enter_record_list.borrow_mut().push(format!("run enter from {:?}", previous_state_name));
/// 	})),
/// 	..StateHandlers::default()
/// });
/// assert_eq!(state_machine.get_state_name(), None);
/// assert!(state_machine.change_state(PlayerState::Idle, false));
/// state_machine.tick(0.5);
/// assert_eq!(state_machine.get_state_seconds(), 0.5);
/// assert!(state_machine.change_state(PlayerState::Run, false));
/// assert_eq!(state_machine.get_state_name(), Some(&PlayerState::Run));
/// assert_eq!(state_machine.get_state_seconds(), 0.0);
/// assert_eq!(*record_list.borrow(), vec!["idle exit to Run", "run enter from Some(Idle)"]);
/// ```
//==============================================================================
pub struct FiniteStateMachine<StateName = String> {
	state_table: HashMap<StateName, StateHandlers<StateName>>,
	state_name: Option<StateName>,
	state_seconds: f64,
	transition_guard: Option<TransitionGuard<StateName>>,
	transition_event: Option<TransitionEvent<StateName>>,
	// 처리 함수가 부탁한 전이. (StateChangeRequester 와 나눠 든다)
	pending_change_list: Rc<RefCell<Vec<(StateName, bool)>>>,
}

impl<StateName: Clone + Eq + Hash> Default for FiniteStateMachine<StateName> {
	//==============================================================================
	/// 빈 상태 기계.
	//==============================================================================
	fn default() -> FiniteStateMachine<StateName> {
		FiniteStateMachine::new()
	}
}

impl<StateName: Clone + Eq + Hash> FiniteStateMachine<StateName> {
	//==============================================================================
	/// 생성. (등록된 상태 없음, 현재 상태 없음)
	///
	/// # 돌려줌
	/// 빈 상태 기계. (현재 상태 None, 머문 시간 0, 가드와 전이 알림 없음)
	//==============================================================================
	pub fn new() -> FiniteStateMachine<StateName> {
		FiniteStateMachine {
			state_table: HashMap::new(),
			state_name: None,
			state_seconds: 0.0,
			transition_guard: None,
			transition_event: None,
			pending_change_list: Rc::new(RefCell::new(Vec::new())),
		}
	}

	//==============================================================================
	/// 처리 함수 안에서 전이를 부탁하는 손잡이. (복제해 처리 함수에 잡아 넣는다)
	///
	/// # 돌려줌
	/// 이 기계의 부탁 목록을 나눠 든 손잡이. (몇 번 받아도 같은 기계를 가리킨다)
	///
	/// # 예제
	/// 처리 함수 밖에서 부탁하면 다음 tick 에 전이한다
	/// ```rust
	/// use vanilla::{FiniteStateMachine, StateHandlers};
	///
	/// let mut state_machine: FiniteStateMachine<&'static str> = FiniteStateMachine::new();
	/// state_machine.add_state("title", StateHandlers::default());
	/// state_machine.add_state("play", StateHandlers::default());
	/// state_machine.change_state("title", false);
	/// let state_change_requester = state_machine.get_state_change_requester();
	/// state_change_requester.change_state("play", false);
	/// assert_eq!(state_machine.get_state_name(), Some(&"title"));
	/// state_machine.tick(0.0);
	/// assert_eq!(state_machine.get_state_name(), Some(&"play"));
	/// ```
	//==============================================================================
	pub fn get_state_change_requester(&self) -> StateChangeRequester<StateName> {
		StateChangeRequester {
			pending_change_list: self.pending_change_list.clone(),
		}
	}

	//==============================================================================
	/// 상태 등록. (같은 이름이 있으면 처리 함수를 바꾼다)
	///
	/// # 인자
	/// - `state_name`: 상태 이름.
	/// - `handlers`: 그 상태의 처리 함수 묶음. (모두 None 이어도 된다, StateHandlers::default())
	//==============================================================================
	pub fn add_state(&mut self, state_name: StateName, handlers: StateHandlers<StateName>) {
		self.state_table.insert(state_name, handlers);
	}

	//==============================================================================
	/// 상태 전이. (전이 성공 여부를 돌려준다)
	/// - 등록되지 않은 상태로는 가지 않는다.
	/// - 같은 상태로의 전이는 기본적으로 무시한다. (is_forced 가 참이면 exit / enter 를 다시 부른다)
	/// - 전이 가드가 거짓을 돌려주면 전이하지 않는다.
	/// - 순서: 이전 상태 exit → 상태 바꾸고 머문 시간 0 → 전이 알림 → 다음 상태 enter.
	/// - 처리 함수가 그 사이에 부탁한 전이는 이 전이가 끝난 뒤 차례대로 한다.
	///
	/// # 인자
	/// - `next_state_name`: 옮겨 갈 상태 이름.
	/// - `is_forced`: 참이면 같은 상태로도 다시 전이한다. (전이 가드는 그대로 본다)
	///
	/// # 돌려줌
	/// 이 전이를 했으면 true. 등록하지 않은 상태, 같은 상태(is_forced 거짓), 가드가 막은 때는 false. (뒤이어 한 부탁받은 전이의 성공 여부는 들지 않는다)
	///
	/// # 예제
	/// 없는 상태, 같은 상태, 강제 전이
	/// ```rust
	/// use vanilla::{FiniteStateMachine, StateHandlers};
	///
	/// let mut state_machine: FiniteStateMachine<&'static str> = FiniteStateMachine::new();
	/// state_machine.add_state("idle", StateHandlers::default());
	/// assert!(!state_machine.change_state("missing", false));
	/// assert!(state_machine.change_state("idle", false));
	/// assert!(!state_machine.change_state("idle", false));
	/// assert!(state_machine.change_state("idle", true));
	/// ```
	//==============================================================================
	pub fn change_state(&mut self, next_state_name: StateName, is_forced: bool) -> bool {
		let is_changed = self.change_state_now(next_state_name, is_forced);
		self.apply_pending_state_changes();
		is_changed
	}

	//==============================================================================
	/// 상태 전이 몸통. (부탁받은 전이는 하지 않는다)
	//==============================================================================
	fn change_state_now(&mut self, next_state_name: StateName, is_forced: bool) -> bool {
		if !self.state_table.contains_key(&next_state_name) {
			return false;
		}
		let previous_state_name = self.state_name.clone();
		if previous_state_name.as_ref() == Some(&next_state_name) && !is_forced {
			return false;
		}
		if let Some(transition_guard) = self.transition_guard.as_mut() {
			if !transition_guard(previous_state_name.as_ref(), &next_state_name) {
				return false;
			}
		}

		if let Some(previous_name) = previous_state_name.as_ref() {
			if let Some(previous_state) = self.state_table.get_mut(previous_name) {
				if let Some(exit) = previous_state.exit.as_mut() {
					exit(&next_state_name);
				}
			}
		}

		self.state_name = Some(next_state_name.clone());
		self.state_seconds = 0.0;

		if let Some(transition_event) = self.transition_event.as_mut() {
			transition_event(previous_state_name.as_ref(), &next_state_name);
		}

		if let Some(next_state) = self.state_table.get_mut(&next_state_name) {
			if let Some(enter) = next_state.enter.as_mut() {
				enter(previous_state_name.as_ref());
			}
		}
		true
	}

	//==============================================================================
	/// 부탁받은 전이를 차례대로 하기. (전이 중에 새로 부탁한 것도 이어서 한다)
	//==============================================================================
	fn apply_pending_state_changes(&mut self) {
		loop {
			let pending_change = {
				let mut pending_change_list = self.pending_change_list.borrow_mut();
				if pending_change_list.is_empty() {
					break;
				}
				pending_change_list.remove(0)
			};
			let (next_state_name, is_forced) = pending_change;
			self.change_state_now(next_state_name, is_forced);
		}
	}

	//==============================================================================
	/// 갱신. (부탁받은 전이를 먼저 하고, 머문 시간을 늘리고 현재 상태의 tick 을 부른다. tick 이 부탁한 전이는 그 뒤에 한다)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임과의 간격. (초, 머문 시간에 더하고 현재 상태의 tick 에 넘긴다. 현재 상태가 없으면 더하지 않는다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f64) {
		self.apply_pending_state_changes();
		self.tick_current_state(time_delta);
		self.apply_pending_state_changes();
	}

	//==============================================================================
	/// 현재 상태의 tick 부르기. (머문 시간을 늘린다)
	//==============================================================================
	fn tick_current_state(&mut self, time_delta: f64) {
		let state_name = match self.state_name.as_ref() {
			Some(state_name) => {
				state_name
			}
			None => {
				return;
			}
		};
		self.state_seconds += time_delta;
		if let Some(current_state) = self.state_table.get_mut(state_name) {
			if let Some(tick) = current_state.tick.as_mut() {
				tick(time_delta, self.state_seconds);
			}
		}
	}

	//==============================================================================
	/// 현재 상태 이름 반환. (아직 아무 상태도 아니면 None)
	///
	/// # 돌려줌
	/// 현재 상태 이름. (아직 전이하지 않았으면 None)
	//==============================================================================
	pub fn get_state_name(&self) -> Option<&StateName> {
		self.state_name.as_ref()
	}

	//==============================================================================
	/// 현재 상태에 머문 시간 반환. (초)
	///
	/// # 돌려줌
	/// 머문 시간. (초, 전이할 때 0 으로 돌아간다)
	//==============================================================================
	pub fn get_state_seconds(&self) -> f64 {
		self.state_seconds
	}

	//==============================================================================
	/// 상태 존재 여부.
	///
	/// # 인자
	/// - `state_name`: 찾을 상태 이름.
	///
	/// # 돌려줌
	/// 등록한 상태면 true.
	//==============================================================================
	pub fn has_state(&self, state_name: &StateName) -> bool {
		self.state_table.contains_key(state_name)
	}

	//==============================================================================
	/// 전이 가드 설정. (previous, next) → bool, None 이면 가드 없음
	///
	/// # 인자
	/// - `transition_guard`: (이전 상태 이름, 다음 상태 이름) 을 받아 허용 여부를 돌려주는 함수. (거짓이면 전이하지 않는다, is_forced 전이도 막는다) None 이면 가드 없음.
	///
	/// # 예제
	/// 쓰러진 뒤로는 전이 막기
	/// ```rust
	/// use vanilla::{FiniteStateMachine, StateHandlers};
	///
	/// #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
	/// enum PlayerState {
	/// 	Idle,
	/// 	Dead,
	/// }
	///
	/// let mut state_machine = FiniteStateMachine::<PlayerState>::new();
	/// state_machine.add_state(PlayerState::Idle, StateHandlers::default());
	/// state_machine.add_state(PlayerState::Dead, StateHandlers::default());
	/// state_machine.set_transition_guard(Some(Box::new(|previous_state_name: Option<&PlayerState>, _next_state_name: &PlayerState| {
	/// 	previous_state_name != Some(&PlayerState::Dead)
	/// })));
	/// assert!(state_machine.change_state(PlayerState::Dead, false));
	/// assert!(!state_machine.change_state(PlayerState::Idle, true));
	/// state_machine.set_transition_guard(None);
	/// assert!(state_machine.change_state(PlayerState::Idle, false));
	/// ```
	//==============================================================================
	pub fn set_transition_guard(&mut self, transition_guard: Option<TransitionGuard<StateName>>) {
		self.transition_guard = transition_guard;
	}

	//==============================================================================
	/// 전이 알림 설정. (previous, next), exit 뒤, enter 앞에 불린다. None 이면 알림 없음
	///
	/// # 인자
	/// - `transition_event`: (이전 상태 이름, 다음 상태 이름) 을 받는 함수. (이전 상태가 없으면 None) None 이면 부르지 않는다.
	///
	/// # 예제
	/// 문자열 상태 이름(기본 타입 String)으로 전이 기록하기
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::{FiniteStateMachine, StateHandlers};
	///
	/// let transition_list = Rc::new(RefCell::new(Vec::<String>::new()));
	/// let transition_list_in_event = transition_list.clone();
	/// let mut state_machine: FiniteStateMachine = FiniteStateMachine::new();
	/// state_machine.add_state(String::from("title"), StateHandlers::default());
	/// state_machine.add_state(String::from("play"), StateHandlers::default());
	/// state_machine.set_transition_event(Some(Box::new(move |previous_state_name: Option<&String>, next_state_name: &String| {
	/// 	transition_list_in_event.borrow_mut().push(format!("{:?} -> {}", previous_state_name, next_state_name));
	/// })));
	/// state_machine.change_state(String::from("title"), false);
	/// state_machine.change_state(String::from("play"), false);
	/// assert_eq!(*transition_list.borrow(), vec!["None -> title", "Some(\"title\") -> play"]);
	/// ```
	//==============================================================================
	pub fn set_transition_event(&mut self, transition_event: Option<TransitionEvent<StateName>>) {
		self.transition_event = transition_event;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
	enum TestState {
		Idle,
		Run,
		Dead,
		Unknown,
	}

	//==============================================================================
	/// 기록을 남기는 처리 함수 묶음을 만든다.
	//==============================================================================
	fn create_recording_handlers(name: &'static str, records: &Rc<RefCell<Vec<String>>>) -> StateHandlers<TestState> {
		let enter_records = records.clone();
		let tick_records = records.clone();
		let exit_records = records.clone();
		StateHandlers {
			enter: Some(Box::new(move |previous_state_name: Option<&TestState>| {
				enter_records.borrow_mut().push(format!("{}.enter({:?})", name, previous_state_name));
			})),
			tick: Some(Box::new(move |time_delta: f64, state_seconds: f64| {
				tick_records.borrow_mut().push(format!("{}.tick({}, {})", name, time_delta, state_seconds));
			})),
			exit: Some(Box::new(move |next_state_name: &TestState| {
				exit_records.borrow_mut().push(format!("{}.exit({:?})", name, next_state_name));
			})),
		}
	}

	//==============================================================================
	/// 전이 순서, 같은 상태 무시와 강제 전이, 머문 시간, 없는 상태, 전이 가드.
	//==============================================================================
	#[test]
	fn transitions() {
		let records = Rc::new(RefCell::new(Vec::<String>::new()));
		let mut state_machine = FiniteStateMachine::<TestState>::new();
		state_machine.add_state(TestState::Idle, create_recording_handlers("idle", &records));
		state_machine.add_state(TestState::Run, create_recording_handlers("run", &records));
		state_machine.add_state(TestState::Dead, StateHandlers::default());
		let event_records = records.clone();
		state_machine.set_transition_event(Some(Box::new(move |previous_state_name: Option<&TestState>, next_state_name: &TestState| {
			event_records.borrow_mut().push(format!("event({:?}, {:?})", previous_state_name, next_state_name));
		})));

		assert!(state_machine.has_state(&TestState::Idle));
		assert!(!state_machine.has_state(&TestState::Unknown));
		assert_eq!(state_machine.get_state_name(), None);
		state_machine.tick(1.0);
		assert_eq!(state_machine.get_state_seconds(), 0.0);
		assert!(!state_machine.change_state(TestState::Unknown, false));

		assert!(state_machine.change_state(TestState::Idle, false));
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Idle));
		state_machine.tick(0.25);
		state_machine.tick(0.5);
		assert_eq!(state_machine.get_state_seconds(), 0.75);
		assert!(!state_machine.change_state(TestState::Idle, false));
		assert_eq!(state_machine.get_state_seconds(), 0.75);
		assert!(state_machine.change_state(TestState::Idle, true));
		assert_eq!(state_machine.get_state_seconds(), 0.0);
		assert!(state_machine.change_state(TestState::Run, false));

		let expected = vec![
			"event(None, Idle)",
			"idle.enter(None)",
			"idle.tick(0.25, 0.25)",
			"idle.tick(0.5, 0.75)",
			"idle.exit(Idle)",
			"event(Some(Idle), Idle)",
			"idle.enter(Some(Idle))",
			"idle.exit(Run)",
			"event(Some(Idle), Run)",
			"run.enter(Some(Idle))",
		];
		assert_eq!(*records.borrow(), expected);

		// 죽으면 어디로도 못 간다.
		state_machine.set_transition_guard(Some(Box::new(|previous_state_name: Option<&TestState>, _next_state_name: &TestState| {
			previous_state_name != Some(&TestState::Dead)
		})));
		assert!(state_machine.change_state(TestState::Dead, false));
		assert!(!state_machine.change_state(TestState::Idle, false));
		assert!(!state_machine.change_state(TestState::Idle, true));
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Dead));
		state_machine.set_transition_guard(None);
		assert!(state_machine.change_state(TestState::Idle, false));
	}

	//==============================================================================
	/// 처리 함수 안에서 부탁한 전이는 그 처리 함수가 끝난 뒤 차례대로 한다. (tick 이 부탁, 그 enter 가 또 부탁)
	//==============================================================================
	#[test]
	fn handlers_request_state_changes() {
		let records = Rc::new(RefCell::new(Vec::<String>::new()));
		let mut state_machine = FiniteStateMachine::<TestState>::new();
		let state_change_requester = state_machine.get_state_change_requester();
		let mut idle_handlers = create_recording_handlers("idle", &records);
		let idle_tick_records = records.clone();
		let idle_requester = state_change_requester.clone();
		idle_handlers.tick = Some(Box::new(move |_time_delta: f64, state_seconds: f64| {
			idle_tick_records.borrow_mut().push(format!("idle.tick({})", state_seconds));
			if state_seconds >= 1.0 {
				idle_requester.change_state(TestState::Run, false);
			}
		}));
		let mut run_handlers = create_recording_handlers("run", &records);
		let run_enter_records = records.clone();
		let run_requester = state_change_requester.clone();
		run_handlers.enter = Some(Box::new(move |previous_state_name: Option<&TestState>| {
			run_enter_records.borrow_mut().push(format!("run.enter({:?})", previous_state_name));
			run_requester.change_state(TestState::Dead, false);
			run_requester.change_state(TestState::Unknown, false);
		}));
		state_machine.add_state(TestState::Idle, idle_handlers);
		state_machine.add_state(TestState::Run, run_handlers);
		state_machine.add_state(TestState::Dead, StateHandlers::default());

		assert!(state_machine.change_state(TestState::Idle, false));
		state_machine.tick(0.5);
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Idle));
		state_machine.tick(0.5);
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Dead));
		let expected = vec![
			"idle.enter(None)",
			"idle.tick(0.5)",
			"idle.tick(1)",
			"idle.exit(Run)",
			"run.enter(Some(Idle))",
			"run.exit(Dead)",
		];
		assert_eq!(*records.borrow(), expected);

		// 처리 함수 밖에서 부탁하면 다음 tick 때 전이한다.
		state_change_requester.change_state(TestState::Idle, false);
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Dead));
		state_machine.tick(0.25);
		assert_eq!(state_machine.get_state_name(), Some(&TestState::Idle));
	}

	//==============================================================================
	/// 원본처럼 문자열 이름으로도 쓴다. (기본 타입 String)
	//==============================================================================
	#[test]
	fn string_state_names() {
		let mut state_machine: FiniteStateMachine = FiniteStateMachine::new();
		state_machine.add_state("title".to_string(), StateHandlers::default());
		state_machine.add_state("play".to_string(), StateHandlers::default());
		assert!(state_machine.change_state("title".to_string(), false));
		assert!(state_machine.change_state("play".to_string(), false));
		assert_eq!(state_machine.get_state_name().map(|state_name| state_name.as_str()), Some("play"));
	}
}
