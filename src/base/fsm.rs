//==============================================================================
// 유한 상태 기계. (원본 base/fsm.js 의 FiniteStateMachine)
// - 여러 게임이 "문자열 상태 + switch 문 + 수동 전이" 를 제각각 만들던 것을 표준화했다.
// - 상태마다 enter / tick / exit 처리를 등록하고 change_state() 로 전이한다.
// - set_transition_guard() 로 허용되지 않은 전이를 막을 수 있다.
// - 원본 상태 이름은 문자열이다. 러스트 판은 상태 이름 타입을 고를 수 있게 했다. (기본은 String, 열거형이나 &'static str 도 된다)
//   원본의 null 상태(아직 아무 상태도 아님)는 None 이다.
// - 원본 handlers 객체 { enter?, tick?, exit? } 는 StateHandlers 구조체다. 콜백은 Box<dyn FnMut> 라서
//   게임 상태를 바꾸려면 Rc<Cell> / Rc<RefCell> 을 잡아 넣는다.
// - 원본은 처리 함수 안에서 같은 기계의 change_state 를 다시 부를 수 있다. 러스트 판은 처리 함수가 기계를 빌릴 수 없어서
//   그렇게 못 한다. 처리 함수가 다음 상태를 표시해 두고, 바깥에서 tick 뒤에 change_state 를 부른다.
//==============================================================================
use std::collections::HashMap;
use std::hash::Hash;

// 상태에 들어갈 때 부른다. (이전 상태 이름, 처음이면 None)
pub type StateEnterHandler<StateName> = Box<dyn FnMut(Option<&StateName>)>;
// 상태에 머무는 동안 tick 마다 부른다. (time_delta, 이 상태에 머문 초)
pub type StateTickHandler = Box<dyn FnMut(f32, f32)>;
// 상태에서 나갈 때 부른다. (다음 상태 이름)
pub type StateExitHandler<StateName> = Box<dyn FnMut(&StateName)>;
// 전이 가드. (이전 상태 이름, 다음 상태 이름) → 허용 여부
pub type TransitionGuard<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName) -> bool>;
// 전이 알림. (이전 상태 이름, 다음 상태 이름) exit 뒤, enter 앞에 불린다.
pub type TransitionEvent<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName)>;

//==============================================================================
// 상태 처리 함수 묶음. (원본 add_state 의 handlers: { enter?, tick?, exit? })
//==============================================================================
pub struct StateHandlers<StateName> {
	pub enter: Option<StateEnterHandler<StateName>>,
	pub tick: Option<StateTickHandler>,
	pub exit: Option<StateExitHandler<StateName>>,
}

impl<StateName> Default for StateHandlers<StateName> {
	//==============================================================================
	// 처리 함수가 하나도 없는 묶음. (원본 기본 인자 handlers = {})
	//==============================================================================
	fn default() -> StateHandlers<StateName> {
		StateHandlers { enter: None, tick: None, exit: None }
	}
}

//==============================================================================
// 유한 상태 기계.
//==============================================================================
pub struct FiniteStateMachine<StateName = String> {
	state_table: HashMap<StateName, StateHandlers<StateName>>,
	state_name: Option<StateName>,
	state_seconds: f32,
	transition_guard: Option<TransitionGuard<StateName>>,
	transition_event: Option<TransitionEvent<StateName>>,
}

impl<StateName: Clone + Eq + Hash> Default for FiniteStateMachine<StateName> {
	//==============================================================================
	// 빈 상태 기계.
	//==============================================================================
	fn default() -> FiniteStateMachine<StateName> {
		FiniteStateMachine::new()
	}
}

impl<StateName: Clone + Eq + Hash> FiniteStateMachine<StateName> {
	//==============================================================================
	// 생성. (등록된 상태 없음, 현재 상태 없음)
	//==============================================================================
	pub fn new() -> FiniteStateMachine<StateName> {
		FiniteStateMachine {
			state_table: HashMap::new(),
			state_name: None,
			state_seconds: 0.0,
			transition_guard: None,
			transition_event: None,
		}
	}

	//==============================================================================
	// 상태 등록. (같은 이름이 있으면 처리 함수를 바꾼다)
	//==============================================================================
	pub fn add_state(&mut self, state_name: StateName, handlers: StateHandlers<StateName>) {
		self.state_table.insert(state_name, handlers);
	}

	//==============================================================================
	// 상태 전이. (전이 성공 여부를 돌려준다)
	// - 등록되지 않은 상태로는 가지 않는다.
	// - 같은 상태로의 전이는 기본적으로 무시한다. (is_forced 가 참이면 exit / enter 를 다시 부른다)
	// - 전이 가드가 거짓을 돌려주면 전이하지 않는다.
	// - 순서: 이전 상태 exit → 상태 바꾸고 머문 시간 0 → 전이 알림 → 다음 상태 enter.
	//==============================================================================
	pub fn change_state(&mut self, next_state_name: StateName, is_forced: bool) -> bool {
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
	// 갱신. (머문 시간을 늘리고 현재 상태의 tick 을 부른다)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
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
	// 현재 상태 이름 반환. (아직 아무 상태도 아니면 None)
	//==============================================================================
	pub fn get_state_name(&self) -> Option<&StateName> {
		self.state_name.as_ref()
	}

	//==============================================================================
	// 현재 상태에 머문 시간 반환. (초)
	//==============================================================================
	pub fn get_state_seconds(&self) -> f32 {
		self.state_seconds
	}

	//==============================================================================
	// 상태 존재 여부.
	//==============================================================================
	pub fn has_state(&self, state_name: &StateName) -> bool {
		self.state_table.contains_key(state_name)
	}

	//==============================================================================
	// 전이 가드 설정. (previous, next) → bool, None 이면 가드 없음
	//==============================================================================
	pub fn set_transition_guard(&mut self, transition_guard: Option<TransitionGuard<StateName>>) {
		self.transition_guard = transition_guard;
	}

	//==============================================================================
	// 전이 알림 설정. (previous, next), exit 뒤, enter 앞에 불린다. None 이면 알림 없음
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
	// 기록을 남기는 처리 함수 묶음을 만든다.
	//==============================================================================
	fn create_recording_handlers(name: &'static str, records: &Rc<RefCell<Vec<String>>>) -> StateHandlers<TestState> {
		let enter_records = records.clone();
		let tick_records = records.clone();
		let exit_records = records.clone();
		StateHandlers {
			enter: Some(Box::new(move |previous_state_name: Option<&TestState>| {
				enter_records.borrow_mut().push(format!("{}.enter({:?})", name, previous_state_name));
			})),
			tick: Some(Box::new(move |time_delta: f32, state_seconds: f32| {
				tick_records.borrow_mut().push(format!("{}.tick({}, {})", name, time_delta, state_seconds));
			})),
			exit: Some(Box::new(move |next_state_name: &TestState| {
				exit_records.borrow_mut().push(format!("{}.exit({:?})", name, next_state_name));
			})),
		}
	}

	//==============================================================================
	// 전이 순서, 같은 상태 무시와 강제 전이, 머문 시간, 없는 상태, 전이 가드.
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
	// 원본처럼 문자열 이름으로도 쓴다. (기본 타입 String)
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
