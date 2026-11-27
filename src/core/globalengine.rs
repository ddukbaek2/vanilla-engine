//==============================================================================
//! 전역 엔진. (원본 Engine.getEngine() 의 System.vanillaEngine 자리. 언리얼 GEngine, 고도 Engine 싱글톤과 같은 꼴)
//! - 엔진은 프로세스의 주 스레드에 하나다. 처음 한 번 만들고 지우지 않는다. 주 스레드 어디서나 Engine::get_engine() 으로 꺼낸다.
//!   원본처럼 마지막으로 돌린 엔진을 준다. (러스트 판은 그 스레드에서 지금 도는 엔진)
//! - 빌림 규칙: 꺼낸 것은 &Engine 이다. (EngineRef, Deref 로 &Engine 처럼 쓴다)
//!   엔진의 &self 함수(매니저 읽기, get_graphic, 시간, 입력, 애셋 부탁, spawn_deferred_task, request_dev_tools_inspect 같은
//!   안쪽 가변성으로 된 것)는 꺼낸 엔진으로 부른다. &mut self 함수는 엔진이 &mut 로 빌려 준 자리(씬 함수의 engine 인자,
//!   with_graphic, 작업 문맥의 with)에서 그 인자로 부른다.
//! - 꺼낼 수 있는 때: 엔진이 &Engine 으로 빌려 준 동안. (컴포넌트 tick, 터치, 휠과 그 안에서 부른 모든 코드:
//!   위젯 콜백, attach_to_tree, attach, detach, 노드 더하기와 지우기, 페이지 이벤트 리스너)
//!   엔진이 &mut 로 빌려 준 동안(씬 함수 create, tick, draw 와 그 안의 컴포넌트 draw, with_graphic, 작업 문맥의 with)에
//!   꺼내면 Err(EngineAccessError::MutablyBorrowed) 다. 그 자리는 인자로 받은 엔진을 쓴다. (두 길이 같은 엔진을 가리킨다)
//!   &mut 로 빌린 엔진이 있는 동안 &Engine 을 또 주면 러스트 빌림 규칙이 깨지므로, 엔진이 &mut 로 고치는 순간에는 꺼낼 수 없게 했다.
//! - 다른 스레드에서 부르면 Err(EngineAccessError::OtherThread) 다. (엔진은 주 스레드 전용, 웹은 스레드가 하나)
//! - EngineRef 는 꺼낸 자리 안에서만 쓴다. 필드나 static 에 넣어 두거나 작업의 await 너머로 들고 가지 않는다.
//!   빌려 준 자리가 끝날 때 꺼낸 것이 살아 있으면 엔진은 까닭을 적고 프로세스를 멈춘다. (그 뒤에 엔진을 &mut 로 고치면 빌림 규칙이 깨지기 때문)
//! - 건전성: 스레드 지역 칸에 빌려 준 자리를 쌓는다. (안쪽이 끝)
//!   1. &Engine 을 빌려 준 자리(Shared)는 그 &Engine 에서 만든 포인터를 든다. 자리가 끝날 때까지 그 &Engine 이 살아 있으니 포인터도 살아 있다.
//!   2. 꺼낸 수를 자리마다 센다. 자리가 끝날 때 0 이 아니면 프로세스를 멈춘다. 그래서 꺼낸 것은 자리보다 오래 쓰이지 않는다.
//!   3. &mut 를 빌려 주는 자리(Mutable)는 같은 엔진을 가리키는 Shared 자리에 꺼낸 것이 남아 있으면 패닉하고 빌려 주지 않는다. (RefCell 의 borrow_mut 과 같다)
//==============================================================================
use crate::core::engine::Engine;
use std::cell::RefCell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicBool, Ordering};

/// 어느 스레드에서든 엔진을 만든 적이 있는지. (다른 스레드에서 꺼낼 때 OtherThread 를 알리려고)
static IS_ANY_ENGINE_CREATED: AtomicBool = AtomicBool::new(false);

//==============================================================================
/// 빌려 준 자리 하나.
//==============================================================================
enum EngineLendScope {
	/// &Engine 을 빌려 준 자리. (그 엔진, 꺼낸 수)
	Shared(NonNull<Engine>, usize),
	/// &mut Engine 을 빌려 준 자리.
	Mutable,
}

//==============================================================================
/// 스레드의 전역 엔진 칸.
//==============================================================================
struct GlobalEngineState {
	/// 이 스레드에서 엔진을 만든 적이 있는지.
	is_engine_created: bool,
	/// 이 스레드에서 도는 러너 수. (러너가 도는 동안 빌려 준 자리 밖은 러너가 엔진을 &mut 로 쥔 때다)
	running_count: usize,
	/// 빌려 준 자리. (안쪽이 끝)
	lend_scope_list: Vec<EngineLendScope>,
}

thread_local! {
	static GLOBAL_ENGINE_STATE: RefCell<GlobalEngineState> = const {
		RefCell::new(GlobalEngineState {
			is_engine_created: false,
			running_count: 0,
			lend_scope_list: Vec::new(),
		})
	};
}

//==============================================================================
/// 전역 엔진을 꺼내지 못한 까닭. (Engine::get_engine)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineAccessError {
	/// 이 스레드에서 엔진을 만든 적이 없다. (원본 getEngine 이 undefined 인 자리)
	NotCreated,
	/// 엔진은 다른 스레드(주 스레드)에 있다. 엔진은 주 스레드에서만 꺼낸다.
	OtherThread,
	/// 엔진이 &mut 로 빌려져 있다. (씬 함수, with_graphic, 작업 문맥의 with 안) 그 자리는 인자로 받은 엔진을 쓴다.
	MutablyBorrowed,
	/// 엔진은 있지만 돌기 전이거나 돌기를 마쳤다. (엔진 값이 Engine::new 를 부른 쪽 손에 있다)
	NotRunning,
}

impl fmt::Display for EngineAccessError {
	//==============================================================================
	/// 까닭 글.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		let message = match self {
			EngineAccessError::NotCreated => "Engine::get_engine: 이 스레드에서 엔진을 만든 적이 없습니다",
			EngineAccessError::OtherThread => "Engine::get_engine: 엔진은 주 스레드에서만 꺼낼 수 있습니다",
			EngineAccessError::MutablyBorrowed => "Engine::get_engine: 엔진이 &mut 로 빌려져 있습니다 (씬 함수, with_graphic, 작업 문맥의 with 안에서는 인자로 받은 엔진을 씁니다)",
			EngineAccessError::NotRunning => "Engine::get_engine: 엔진이 돌기 전이거나 돌기를 마쳤습니다 (엔진 값을 쥔 쪽에서 씁니다)",
		};
		formatter.write_str(message)
	}
}

impl std::error::Error for EngineAccessError {}

//==============================================================================
/// 전역 엔진에서 꺼낸 &Engine. (Engine::get_engine, Deref 로 &Engine 처럼 쓴다)
/// - 꺼낸 자리 안에서만 쓴다. 필드나 static 에 넣어 두거나 await 너머로 들고 가지 않는다. (빌려 준 자리가 끝날 때 살아 있으면 엔진이 프로세스를 멈춘다)
/// - Send, Sync 가 아니다. (다른 스레드로 보낼 수 없다)
//==============================================================================
pub struct EngineRef {
	engine: NonNull<Engine>,
	lend_scope_index: usize,
	_not_send: PhantomData<*const ()>,
}

impl Deref for EngineRef {
	type Target = Engine;

	//==============================================================================
	/// 엔진.
	//==============================================================================
	fn deref(&self) -> &Engine {
		// SAFETY: 포인터는 빌려 준 자리(Shared)의 &Engine 에서 만들었다. 이 값이 살아 있는 동안 그 자리의 꺼낸 수가 0 이 아니어서
		// 자리는 끝나지 않고(끝나면 프로세스를 멈춘다), 같은 엔진을 &mut 로 빌려 주지도 않는다(Mutable 자리가 패닉한다).
		unsafe { self.engine.as_ref() }
	}
}

impl Drop for EngineRef {
	//==============================================================================
	/// 꺼낸 수 줄이기.
	//==============================================================================
	fn drop(&mut self) {
		let lend_scope_index = self.lend_scope_index;
		let _ = GLOBAL_ENGINE_STATE.try_with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			if let Some(EngineLendScope::Shared(_, reference_count)) = state.lend_scope_list.get_mut(lend_scope_index) {
				*reference_count -= 1;
			}
		});
	}
}

impl fmt::Debug for EngineRef {
	//==============================================================================
	/// 디버그 글.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		formatter.write_str("EngineRef")
	}
}

//==============================================================================
/// 엔진을 만들었다고 적기. (Engine::new)
//==============================================================================
pub(crate) fn mark_engine_created() {
	IS_ANY_ENGINE_CREATED.store(true, Ordering::Relaxed);
	GLOBAL_ENGINE_STATE.with(|state_cell| state_cell.borrow_mut().is_engine_created = true);
}

//==============================================================================
/// 꺼내기. (Engine::get_engine)
//==============================================================================
pub(crate) fn get_global_engine() -> Result<EngineRef, EngineAccessError> {
	let result = GLOBAL_ENGINE_STATE.try_with(|state_cell| {
		let mut state = state_cell.borrow_mut();
		let lend_scope_index = state.lend_scope_list.len().wrapping_sub(1);
		match state.lend_scope_list.last_mut() {
			Some(EngineLendScope::Shared(engine, reference_count)) => {
				*reference_count += 1;
				Ok(EngineRef {
					engine: *engine,
					lend_scope_index,
					_not_send: PhantomData,
				})
			}
			Some(EngineLendScope::Mutable) => {
				Err(EngineAccessError::MutablyBorrowed)
			}
			None => {
				if state.running_count > 0 {
					Err(EngineAccessError::MutablyBorrowed)
				}
				else if state.is_engine_created {
					Err(EngineAccessError::NotRunning)
				}
				else if IS_ANY_ENGINE_CREATED.load(Ordering::Relaxed) {
					Err(EngineAccessError::OtherThread)
				}
				else {
					Err(EngineAccessError::NotCreated)
				}
			}
		}
	});
	match result {
		Ok(result) => {
			result
		}
		Err(_) => {
			Err(EngineAccessError::NotRunning)
		}
	}
}

//==============================================================================
/// &Engine 을 빌려 준 자리. (만들 때 칸에 쌓고, 버릴 때 꺼낸 것이 남았으면 프로세스를 멈춘다)
//==============================================================================
pub(crate) struct SharedEngineScope {
	lend_scope_index: usize,
	_engine: PhantomData<*const Engine>,
}

impl SharedEngineScope {
	//==============================================================================
	/// 빌려 주기.
	///
	/// # 인자
	/// - `engine`: 빌려 줄 엔진. (돌려준 값을 버릴 때까지 살아 있어야 한다. 그래서 그 &Engine 을 쥔 함수 안에서 만들고 버린다)
	//==============================================================================
	pub(crate) fn enter(engine: &Engine) -> SharedEngineScope {
		let engine = NonNull::from(engine);
		let lend_scope_index = GLOBAL_ENGINE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			state.lend_scope_list.push(EngineLendScope::Shared(engine, 0));
			state.lend_scope_list.len() - 1
		});
		SharedEngineScope {
			lend_scope_index,
			_engine: PhantomData,
		}
	}
}

impl Drop for SharedEngineScope {
	//==============================================================================
	/// 거두기.
	//==============================================================================
	fn drop(&mut self) {
		let lend_scope_index = self.lend_scope_index;
		let reference_count = GLOBAL_ENGINE_STATE.try_with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			let reference_count = match state.lend_scope_list.get(lend_scope_index) {
				Some(EngineLendScope::Shared(_, reference_count)) => {
					*reference_count
				}
				_ => {
					0
				}
			};
			state.lend_scope_list.truncate(lend_scope_index);
			reference_count
		});
		if let Ok(reference_count) = reference_count {
			if reference_count > 0 {
				eprintln!("Engine::get_engine: 꺼낸 엔진(EngineRef {} 개)이 빌려 준 자리 밖에 남았습니다. 필드나 static 에 넣거나 await 너머로 들고 가지 않습니다. 빌림 규칙을 지키려고 프로세스를 멈춥니다", reference_count);
				std::process::abort();
			}
		}
	}
}

//==============================================================================
/// &mut Engine 을 빌려 준 자리. (만들 때 같은 엔진에 꺼낸 것이 남았으면 패닉한다)
//==============================================================================
pub(crate) struct MutableEngineScope {
	lend_scope_index: usize,
	_engine: PhantomData<*const Engine>,
}

impl MutableEngineScope {
	//==============================================================================
	/// 빌려 주기.
	///
	/// # 인자
	/// - `engine`: &mut 로 빌려 줄 엔진.
	/// - `function_name`: 패닉 글에 적을 함수 이름.
	//==============================================================================
	#[track_caller]
	pub(crate) fn enter(engine: NonNull<Engine>, function_name: &str) -> MutableEngineScope {
		let lend_scope_index = GLOBAL_ENGINE_STATE.with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			let is_shared_reference_alive = state.lend_scope_list.iter().any(|lend_scope| {
				match lend_scope {
					EngineLendScope::Shared(shared_engine, reference_count) => {
						*shared_engine == engine && *reference_count > 0
					}
					EngineLendScope::Mutable => {
						false
					}
				}
			});
			if is_shared_reference_alive {
				None
			}
			else {
				state.lend_scope_list.push(EngineLendScope::Mutable);
				Some(state.lend_scope_list.len() - 1)
			}
		});
		match lend_scope_index {
			Some(lend_scope_index) => {
				MutableEngineScope {
					lend_scope_index,
					_engine: PhantomData,
				}
			}
			None => {
				panic!("{}: Engine::get_engine 으로 꺼낸 엔진(EngineRef)이 살아 있는 동안에는 엔진을 &mut 로 빌릴 수 없습니다", function_name);
			}
		}
	}
}

impl Drop for MutableEngineScope {
	//==============================================================================
	/// 거두기.
	//==============================================================================
	fn drop(&mut self) {
		let lend_scope_index = self.lend_scope_index;
		let _ = GLOBAL_ENGINE_STATE.try_with(|state_cell| state_cell.borrow_mut().lend_scope_list.truncate(lend_scope_index));
	}
}

//==============================================================================
/// 러너가 도는 자리. (만들 때 늘리고 버릴 때 줄인다. 그동안 빌려 준 자리 밖은 러너가 엔진을 &mut 로 쥔 때다)
//==============================================================================
pub(crate) struct EngineRunningScope {
	_not_send: PhantomData<*const ()>,
}

impl EngineRunningScope {
	//==============================================================================
	/// 돌기 시작.
	//==============================================================================
	pub(crate) fn enter() -> EngineRunningScope {
		GLOBAL_ENGINE_STATE.with(|state_cell| state_cell.borrow_mut().running_count += 1);
		EngineRunningScope {
			_not_send: PhantomData,
		}
	}
}

impl Drop for EngineRunningScope {
	//==============================================================================
	/// 돌기 마침.
	//==============================================================================
	fn drop(&mut self) {
		let _ = GLOBAL_ENGINE_STATE.try_with(|state_cell| {
			let mut state = state_cell.borrow_mut();
			state.running_count = state.running_count.saturating_sub(1);
		});
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{EngineConfiguration, EngineRunner, PageEvent, PageEventListener};
	use crate::core::node::nodetree::{ComponentContext, NodeTree};
	use crate::core::scene::{Scene, SceneBase};
	use std::cell::Cell;
	use std::rc::Rc;

	/// 엔진을 받지 않는 함수. (어디서나 꺼내는 자리)
	fn read_global_engine_address() -> Result<usize, EngineAccessError> {
		let engine = Engine::get_engine()?;
		Ok(&*engine as *const Engine as usize)
	}

	struct GlobalReader {
		component_base: ComponentBase,
		read_list: Rc<RefCell<Vec<Result<usize, EngineAccessError>>>>,
	}

	#[crate::reflect]
	impl GlobalReader {}

	#[crate::reflect]
	impl Component for GlobalReader {
		fn get_component_base(&self) -> &ComponentBase {
			&self.component_base
		}

		fn get_component_base_mut(&mut self) -> &mut ComponentBase {
			&mut self.component_base
		}

		fn get_component_type(&self) -> &'static str {
			"GlobalReader"
		}

		fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
			let read = read_global_engine_address();
			// 문맥의 엔진과 전역 엔진은 같은 엔진이다.
			assert_eq!(read, Ok(context.get_engine() as *const Engine as usize));
			self.read_list.borrow_mut().push(read);
		}
	}

	#[test]
	fn get_engine_inside_component_tick_from_function_without_context() {
		let engine = Engine::new(EngineConfiguration::new());
		assert_eq!(Engine::get_engine().unwrap_err(), EngineAccessError::NotRunning);
		let read_list = Rc::new(RefCell::new(Vec::new()));
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let reader_id = node_tree.create_child(root_id, "reader");
		node_tree[reader_id].add_component(GlobalReader {
			component_base: ComponentBase::new(),
			read_list: Rc::clone(&read_list),
		});
		node_tree.tick(&engine, 0.016);
		node_tree.tick(&engine, 0.016);
		let engine_address = &engine as *const Engine as usize;
		assert_eq!(*read_list.borrow(), vec![Ok(engine_address), Ok(engine_address)]);
		// 빌려 준 자리가 끝나면 다시 못 꺼낸다.
		assert_eq!(Engine::get_engine().unwrap_err(), EngineAccessError::NotRunning);
	}

	#[test]
	fn nested_shared_scope_and_reference_dropped_in_inner_scope() {
		let engine = Engine::new(EngineConfiguration::new());
		let outer_scope = SharedEngineScope::enter(&engine);
		let outer_reference = Engine::get_engine().unwrap();
		{
			let inner_scope = SharedEngineScope::enter(&engine);
			let inner_reference = Engine::get_engine().unwrap();
			assert_eq!(inner_reference.get_frame_number(), 1);
			drop(outer_reference);
			drop(inner_reference);
			drop(inner_scope);
		}
		assert_eq!(Engine::get_engine().unwrap().get_frame_number(), 1);
		drop(outer_scope);
		assert_eq!(Engine::get_engine().unwrap_err(), EngineAccessError::NotRunning);
	}

	#[test]
	#[should_panic(expected = "엔진을 &mut 로 빌릴 수 없습니다")]
	fn mutable_lend_while_engine_reference_alive_panics() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let engine_pointer = NonNull::from(&mut engine);
		let shared_engine: &Engine = &engine;
		let _shared_scope = SharedEngineScope::enter(shared_engine);
		let _engine_reference = Engine::get_engine().unwrap();
		let _mutable_scope = MutableEngineScope::enter(engine_pointer, "test");
	}

	#[test]
	fn mutable_lend_inside_shared_scope_reports_mutably_borrowed() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let engine_pointer = NonNull::from(&mut engine);
		let shared_scope = SharedEngineScope::enter(&engine);
		{
			let _mutable_scope = MutableEngineScope::enter(engine_pointer, "test");
			assert_eq!(Engine::get_engine().unwrap_err(), EngineAccessError::MutablyBorrowed);
		}
		assert!(Engine::get_engine().is_ok());
		drop(shared_scope);
	}

	struct RecordingScene {
		scene_base: SceneBase,
		create_read: Rc<Cell<Option<Result<usize, EngineAccessError>>>>,
		task_read: Rc<RefCell<Vec<Result<usize, EngineAccessError>>>>,
	}

	impl Scene for RecordingScene {
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		fn create(&mut self, engine: &mut Engine) {
			// 씬 함수는 엔진이 &mut 로 빌려 준 자리다.
			self.create_read.set(Some(read_global_engine_address()));
			assert_eq!(self.scene_base.get_engine().unwrap_err(), EngineAccessError::MutablyBorrowed);
			let task_read = Rc::clone(&self.task_read);
			engine.spawn_task(move |mut context| async move {
				task_read.borrow_mut().push(read_global_engine_address());
				let inner_read = context.with(|_engine| read_global_engine_address());
				task_read.borrow_mut().push(inner_read);
			});
		}
	}

	#[test]
	fn scene_function_and_task_report_mutably_borrowed() {
		let create_read = Rc::new(Cell::new(None));
		let task_read = Rc::new(RefCell::new(Vec::new()));
		let scene = RecordingScene {
			scene_base: SceneBase::new(),
			create_read: Rc::clone(&create_read),
			task_read: Rc::clone(&task_read),
		};
		let mut engine_runner = EngineRunner::new(Engine::new(EngineConfiguration::new()), Box::new(scene));
		engine_runner.apply_pending_scenes();
		assert_eq!(create_read.get(), Some(Err(EngineAccessError::MutablyBorrowed)));
		assert_eq!(*task_read.borrow(), vec![Err(EngineAccessError::MutablyBorrowed), Err(EngineAccessError::MutablyBorrowed)]);
		drop(engine_runner);
		assert_eq!(Engine::get_engine().unwrap_err(), EngineAccessError::NotRunning);
	}

	#[test]
	fn page_event_listener_gets_engine() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let engine_address = &engine as *const Engine as usize;
		let read = Rc::new(Cell::new(None));
		let listener_read = Rc::clone(&read);
		let page_event_listener: PageEventListener = Rc::new(RefCell::new(move |_page_event: PageEvent| {
			listener_read.set(Some(read_global_engine_address()));
		}));
		engine.add_page_event_listener(&page_event_listener);
		engine.dispatch_page_event(PageEvent::Focused);
		assert_eq!(read.get(), Some(Ok(engine_address)));
	}

	#[test]
	fn other_thread_gets_other_thread_error() {
		let _engine = Engine::new(EngineConfiguration::new());
		let result = std::thread::spawn(|| Engine::get_engine().map(|_| ())).join().unwrap();
		assert_eq!(result, Err(EngineAccessError::OtherThread));
	}

	#[test]
	fn counterpart_names_match_original() {
		let counterpart_text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/api/counterpart.0.4.0.json")).unwrap();
		let counterpart: serde_json::Value = serde_json::from_str(&counterpart_text).unwrap();
		let item_list = counterpart["items"].as_array().unwrap();
		let has_pair = |rust_class: &str, original_class: &str| {
			item_list.iter().any(|item| {
				item["vanilla-engine"]["class"] == rust_class && item["vanilla-engine"]["name"] == "get_engine" && item["vanilla.js"]["class"] == original_class && item["vanilla.js"]["name"] == "getEngine"
			})
		};
		assert!(has_pair("Engine", "Engine"));
		assert!(has_pair("SceneBase", "Scene"));
		assert!(has_pair("UIView", "UIView"));
	}
}
