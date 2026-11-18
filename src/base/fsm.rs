//==============================================================================
// 유한 상태 기계. (원본 base/fsm.js 의 FiniteStateMachine)
// - 여러 게임이 "문자열 상태 + switch 문 + 수동 전이" 를 제각각 만들던 것을 표준화했다.
// - 상태마다 enter / tick / exit 처리를 등록하고 changeState() 로 전이한다.
// - setTransitionGuard() 로 허용되지 않은 전이를 막을 수 있다.
// - 원본 상태 이름은 문자열이다. 러스트 판은 상태 이름 타입을 고를 수 있게 했다. (기본은 String, 열거형이나 &'static str 도 된다)
//   원본의 null 상태(아직 아무 상태도 아님)는 None 이다.
// - 원본 handlers 객체 { enter?, tick?, exit? } 는 StateHandlers 구조체다. 콜백은 Box<dyn FnMut> 라서
//   게임 상태를 바꾸려면 Rc<Cell> / Rc<RefCell> 을 잡아 넣는다.
// - 원본은 처리 함수 안에서 같은 기계의 changeState 를 다시 부를 수 있다. 러스트 판은 처리 함수가 기계를 빌릴 수 없어서
//   그렇게 못 한다. 처리 함수가 다음 상태를 표시해 두고, 바깥에서 tick 뒤에 changeState 를 부른다.
//==============================================================================
use std::collections::HashMap;
use std::hash::Hash;

// 상태에 들어갈 때 부른다. (이전 상태 이름, 처음이면 None)
pub type StateEnterHandler<StateName> = Box<dyn FnMut(Option<&StateName>)>;
// 상태에 머무는 동안 tick 마다 부른다. (timeDelta, 이 상태에 머문 초)
pub type StateTickHandler = Box<dyn FnMut(f32, f32)>;
// 상태에서 나갈 때 부른다. (다음 상태 이름)
pub type StateExitHandler<StateName> = Box<dyn FnMut(&StateName)>;
// 전이 가드. (이전 상태 이름, 다음 상태 이름) → 허용 여부
pub type TransitionGuard<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName) -> bool>;
// 전이 알림. (이전 상태 이름, 다음 상태 이름) exit 뒤, enter 앞에 불린다.
pub type TransitionEvent<StateName> = Box<dyn FnMut(Option<&StateName>, &StateName)>;

//==============================================================================
// 상태 처리 함수 묶음. (원본 addState 의 handlers: { enter?, tick?, exit? })
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
	stateTable: HashMap<StateName, StateHandlers<StateName>>,
	stateName: Option<StateName>,
	stateSeconds: f32,
	transitionGuard: Option<TransitionGuard<StateName>>,
	transitionEvent: Option<TransitionEvent<StateName>>,
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
			stateTable: HashMap::new(),
			stateName: None,
			stateSeconds: 0.0,
			transitionGuard: None,
			transitionEvent: None,
		}
	}

	//==============================================================================
	// 상태 등록. (같은 이름이 있으면 처리 함수를 바꾼다)
	//==============================================================================
	pub fn addState(&mut self, stateName: StateName, handlers: StateHandlers<StateName>) {
		self.stateTable.insert(stateName, handlers);
	}

	//==============================================================================
	// 상태 전이. (전이 성공 여부를 돌려준다)
	// - 등록되지 않은 상태로는 가지 않는다.
	// - 같은 상태로의 전이는 기본적으로 무시한다. (isForced 가 참이면 exit / enter 를 다시 부른다)
	// - 전이 가드가 거짓을 돌려주면 전이하지 않는다.
	// - 순서: 이전 상태 exit → 상태 바꾸고 머문 시간 0 → 전이 알림 → 다음 상태 enter.
	//==============================================================================
	pub fn changeState(&mut self, nextStateName: StateName, isForced: bool) -> bool {
		if !self.stateTable.contains_key(&nextStateName) {
			return false;
		}
		let previousStateName = self.stateName.clone();
		if previousStateName.as_ref() == Some(&nextStateName) && !isForced {
			return false;
		}
		if let Some(transitionGuard) = self.transitionGuard.as_mut() {
			if !transitionGuard(previousStateName.as_ref(), &nextStateName) {
				return false;
			}
		}

		if let Some(previousName) = previousStateName.as_ref() {
			if let Some(previousState) = self.stateTable.get_mut(previousName) {
				if let Some(exit) = previousState.exit.as_mut() {
					exit(&nextStateName);
				}
			}
		}

		self.stateName = Some(nextStateName.clone());
		self.stateSeconds = 0.0;

		if let Some(transitionEvent) = self.transitionEvent.as_mut() {
			transitionEvent(previousStateName.as_ref(), &nextStateName);
		}

		if let Some(nextState) = self.stateTable.get_mut(&nextStateName) {
			if let Some(enter) = nextState.enter.as_mut() {
				enter(previousStateName.as_ref());
			}
		}
		true
	}

	//==============================================================================
	// 갱신. (머문 시간을 늘리고 현재 상태의 tick 을 부른다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		let stateName = match self.stateName.as_ref() {
			Some(stateName) => {
				stateName
			}
			None => {
				return;
			}
		};
		self.stateSeconds += timeDelta;
		if let Some(currentState) = self.stateTable.get_mut(stateName) {
			if let Some(tick) = currentState.tick.as_mut() {
				tick(timeDelta, self.stateSeconds);
			}
		}
	}

	//==============================================================================
	// 현재 상태 이름 반환. (아직 아무 상태도 아니면 None)
	//==============================================================================
	pub fn getStateName(&self) -> Option<&StateName> {
		self.stateName.as_ref()
	}

	//==============================================================================
	// 현재 상태에 머문 시간 반환. (초)
	//==============================================================================
	pub fn getStateSeconds(&self) -> f32 {
		self.stateSeconds
	}

	//==============================================================================
	// 상태 존재 여부.
	//==============================================================================
	pub fn hasState(&self, stateName: &StateName) -> bool {
		self.stateTable.contains_key(stateName)
	}

	//==============================================================================
	// 전이 가드 설정. (previous, next) → bool, None 이면 가드 없음
	//==============================================================================
	pub fn setTransitionGuard(&mut self, transitionGuard: Option<TransitionGuard<StateName>>) {
		self.transitionGuard = transitionGuard;
	}

	//==============================================================================
	// 전이 알림 설정. (previous, next), exit 뒤, enter 앞에 불린다. None 이면 알림 없음
	//==============================================================================
	pub fn setTransitionEvent(&mut self, transitionEvent: Option<TransitionEvent<StateName>>) {
		self.transitionEvent = transitionEvent;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::RefCell;
	use std::rc::Rc;

	#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
	enum TestState {
		idle,
		run,
		dead,
		unknown,
	}

	//==============================================================================
	// 기록을 남기는 처리 함수 묶음을 만든다.
	//==============================================================================
	fn createRecordingHandlers(name: &'static str, records: &Rc<RefCell<Vec<String>>>) -> StateHandlers<TestState> {
		let enterRecords = records.clone();
		let tickRecords = records.clone();
		let exitRecords = records.clone();
		StateHandlers {
			enter: Some(Box::new(move |previousStateName: Option<&TestState>| {
				enterRecords.borrow_mut().push(format!("{}.enter({:?})", name, previousStateName));
			})),
			tick: Some(Box::new(move |timeDelta: f32, stateSeconds: f32| {
				tickRecords.borrow_mut().push(format!("{}.tick({}, {})", name, timeDelta, stateSeconds));
			})),
			exit: Some(Box::new(move |nextStateName: &TestState| {
				exitRecords.borrow_mut().push(format!("{}.exit({:?})", name, nextStateName));
			})),
		}
	}

	//==============================================================================
	// 전이 순서, 같은 상태 무시와 강제 전이, 머문 시간, 없는 상태, 전이 가드.
	//==============================================================================
	#[test]
	fn transitions() {
		let records = Rc::new(RefCell::new(Vec::<String>::new()));
		let mut stateMachine = FiniteStateMachine::<TestState>::new();
		stateMachine.addState(TestState::idle, createRecordingHandlers("idle", &records));
		stateMachine.addState(TestState::run, createRecordingHandlers("run", &records));
		stateMachine.addState(TestState::dead, StateHandlers::default());
		let eventRecords = records.clone();
		stateMachine.setTransitionEvent(Some(Box::new(move |previousStateName: Option<&TestState>, nextStateName: &TestState| {
			eventRecords.borrow_mut().push(format!("event({:?}, {:?})", previousStateName, nextStateName));
		})));

		assert!(stateMachine.hasState(&TestState::idle));
		assert!(!stateMachine.hasState(&TestState::unknown));
		assert_eq!(stateMachine.getStateName(), None);
		stateMachine.tick(1.0);
		assert_eq!(stateMachine.getStateSeconds(), 0.0);
		assert!(!stateMachine.changeState(TestState::unknown, false));

		assert!(stateMachine.changeState(TestState::idle, false));
		assert_eq!(stateMachine.getStateName(), Some(&TestState::idle));
		stateMachine.tick(0.25);
		stateMachine.tick(0.5);
		assert_eq!(stateMachine.getStateSeconds(), 0.75);
		assert!(!stateMachine.changeState(TestState::idle, false));
		assert_eq!(stateMachine.getStateSeconds(), 0.75);
		assert!(stateMachine.changeState(TestState::idle, true));
		assert_eq!(stateMachine.getStateSeconds(), 0.0);
		assert!(stateMachine.changeState(TestState::run, false));

		let expected = vec![
			"event(None, idle)",
			"idle.enter(None)",
			"idle.tick(0.25, 0.25)",
			"idle.tick(0.5, 0.75)",
			"idle.exit(idle)",
			"event(Some(idle), idle)",
			"idle.enter(Some(idle))",
			"idle.exit(run)",
			"event(Some(idle), run)",
			"run.enter(Some(idle))",
		];
		assert_eq!(*records.borrow(), expected);

		// 죽으면 어디로도 못 간다.
		stateMachine.setTransitionGuard(Some(Box::new(|previousStateName: Option<&TestState>, _nextStateName: &TestState| {
			previousStateName != Some(&TestState::dead)
		})));
		assert!(stateMachine.changeState(TestState::dead, false));
		assert!(!stateMachine.changeState(TestState::idle, false));
		assert!(!stateMachine.changeState(TestState::idle, true));
		assert_eq!(stateMachine.getStateName(), Some(&TestState::dead));
		stateMachine.setTransitionGuard(None);
		assert!(stateMachine.changeState(TestState::idle, false));
	}

	//==============================================================================
	// 원본처럼 문자열 이름으로도 쓴다. (기본 타입 String)
	//==============================================================================
	#[test]
	fn stringStateNames() {
		let mut stateMachine: FiniteStateMachine = FiniteStateMachine::new();
		stateMachine.addState("title".to_string(), StateHandlers::default());
		stateMachine.addState("play".to_string(), StateHandlers::default());
		assert!(stateMachine.changeState("title".to_string(), false));
		assert!(stateMachine.changeState("play".to_string(), false));
		assert_eq!(stateMachine.getStateName().map(|stateName| stateName.as_str()), Some("play"));
	}
}
