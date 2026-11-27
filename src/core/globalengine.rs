//==============================================================================
//! 전역 엔진. (원본 Engine.getEngine() 의 System.vanillaEngine 자리. 언리얼 GEngine, 고도 Engine 싱글톤과 같은 꼴)
//! - 엔진은 처음 한 번 만들고 지우지 않는다. Engine::new 가 엔진을 프로그램 끝까지 사는 자리('static)에 두고 &'static Engine 을 준다.
//!   그 자리를 스레드 칸에 적어 두어, 만든 직후부터 씬 함수, draw, 컴포넌트, 위젯 콜백, async 작업 안 어디서나 Engine::get_engine() 이 같은 엔진을 준다.
//!   원본처럼 마지막으로 만든 엔진을 준다.
//! - 엔진의 공개 함수는 모두 &self 다. 고치는 칸은 안쪽 가변성(Cell, RefCell)으로 둔다. (고도의 러스트 연결 godot-rust 싱글톤과 Gd 의 bind, bind_mut 꼴)
//!   그래서 씬 함수가 받은 engine 인자와 Engine::get_engine() 이 같은 엔진을 동시에 써도 러스트 빌림 규칙이 깨지지 않는다.
//! - 매니저(시간, 뷰, 입력, 포인터, 소리, 그래픽)는 RefCell 안에 있다. get_x 는 Ref, get_x_mut 은 RefMut 을 준다.
//!   같은 매니저를 RefMut 으로 쥔 채 또 빌리면(get_x 나 get_x_mut) 함수 이름과 매니저 이름을 적은 패닉으로 멈춘다.
//!   빌린 것은 한 문장 안에서 쓰고 버린다. (let 으로 오래 쥐고 다른 엔진 함수를 부르지 않는다)
//! - 다른 스레드에서 부르면 Err(EngineAccessError::OtherThread) 다. (엔진은 만든 스레드 전용이다. Engine 은 Sync 가 아니라 &Engine 을 다른 스레드로 보낼 수 없다. 웹은 스레드가 하나)
//==============================================================================
use crate::core::engine::Engine;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// 어느 스레드에서든 엔진을 만든 적이 있는지. (다른 스레드에서 꺼낼 때 OtherThread 를 알리려고)
static IS_ANY_ENGINE_CREATED: AtomicBool = AtomicBool::new(false);

thread_local! {
	/// 이 스레드에서 마지막으로 만든 엔진.
	static GLOBAL_ENGINE: Cell<Option<&'static Engine>> = const { Cell::new(None) };
}

//==============================================================================
/// 전역 엔진을 꺼내지 못한 까닭. (Engine::get_engine)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineAccessError {
	/// 엔진을 만든 적이 없다. (원본 getEngine 이 undefined 인 자리)
	NotCreated,
	/// 엔진은 다른 스레드(엔진을 만든 주 스레드)에 있다. 엔진은 그 스레드에서만 꺼낸다.
	OtherThread,
}

impl fmt::Display for EngineAccessError {
	//==============================================================================
	/// 까닭 글.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		let message = match self {
			EngineAccessError::NotCreated => "Engine::get_engine: 엔진을 만든 적이 없습니다",
			EngineAccessError::OtherThread => "Engine::get_engine: 엔진은 엔진을 만든 스레드에서만 꺼낼 수 있습니다",
		};
		formatter.write_str(message)
	}
}

impl std::error::Error for EngineAccessError {}

//==============================================================================
/// 엔진을 프로그램 끝까지 사는 자리에 두고 전역 엔진으로 적기. (Engine::new)
/// - 엔진은 지우지 않는다. (원본 System.vanillaEngine 처럼 프로세스 끝까지 산다)
//==============================================================================
pub(crate) fn install_global_engine(engine: Engine) -> &'static Engine {
	let engine: &'static Engine = Box::leak(Box::new(engine));
	IS_ANY_ENGINE_CREATED.store(true, Ordering::Relaxed);
	GLOBAL_ENGINE.with(|engine_cell| engine_cell.set(Some(engine)));
	engine
}

//==============================================================================
/// 꺼내기. (Engine::get_engine)
//==============================================================================
pub(crate) fn get_global_engine() -> Result<&'static Engine, EngineAccessError> {
	let engine = GLOBAL_ENGINE.try_with(|engine_cell| engine_cell.get()).ok().flatten();
	match engine {
		Some(engine) => {
			Ok(engine)
		}
		None => {
			if IS_ANY_ENGINE_CREATED.load(Ordering::Relaxed) {
				Err(EngineAccessError::OtherThread)
			}
			else {
				Err(EngineAccessError::NotCreated)
			}
		}
	}
}

//==============================================================================
/// 엔진 칸 읽기 빌림. (겹쳐 빌리면 함수 이름과 칸 이름을 적고 패닉한다)
//==============================================================================
#[track_caller]
pub(crate) fn borrow_engine_field<'a, T: ?Sized>(field_cell: &'a RefCell<T>, function_name: &str, field_name: &str) -> Ref<'a, T> {
	match field_cell.try_borrow() {
		Ok(field) => {
			field
		}
		Err(_) => {
			panic!("{function_name}: 엔진의 {field_name} 칸을 이미 고치는 중이라 빌릴 수 없습니다 (get_x_mut 으로 받은 것을 쥔 채 같은 칸을 또 빌렸습니다. 빌린 것은 한 문장 안에서 쓰고 버립니다)");
		}
	}
}

//==============================================================================
/// 엔진 칸 고치기 빌림. (겹쳐 빌리면 함수 이름과 칸 이름을 적고 패닉한다)
//==============================================================================
#[track_caller]
pub(crate) fn borrow_engine_field_mut<'a, T: ?Sized>(field_cell: &'a RefCell<T>, function_name: &str, field_name: &str) -> RefMut<'a, T> {
	match field_cell.try_borrow_mut() {
		Ok(field) => {
			field
		}
		Err(_) => {
			panic!("{function_name}: 엔진의 {field_name} 칸을 이미 빌려 쓰는 중이라 고칠 수 없습니다 (get_x 나 get_x_mut 으로 받은 것을 쥔 채 같은 칸을 또 빌렸습니다. 빌린 것은 한 문장 안에서 쓰고 버립니다)");
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::component::{Component, ComponentBase};
	use crate::core::engine::{EngineConfiguration, EngineRunner, PageEvent, PageEventListener};
	use crate::core::node::nodetree::{ComponentContext, NodeTree};
	use crate::core::scene::{Scene, SceneBase};
	use crate::core::task::{self, Promise};
	use std::rc::Rc;

	/// 엔진을 받지 않는 함수. (어디서나 꺼내는 자리)
	fn read_global_engine_address() -> Result<usize, EngineAccessError> {
		let engine = Engine::get_engine()?;
		Ok(engine as *const Engine as usize)
	}

	fn get_engine_address(engine: &Engine) -> usize {
		engine as *const Engine as usize
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
			assert_eq!(read, Ok(get_engine_address(context.get_engine())));
			// 꺼낸 엔진으로 고치는 함수도 부른다. (엔진 함수는 모두 &self)
			Engine::get_engine().unwrap().get_time_manager_mut().set_time_scale(1.0);
			self.read_list.borrow_mut().push(read);
		}
	}

	#[test]
	fn get_engine_right_after_new() {
		let engine = Engine::new(EngineConfiguration::new());
		assert!(std::ptr::eq(Engine::get_engine().unwrap(), engine));
		// 마지막으로 만든 엔진을 준다. (원본 System.vanillaEngine)
		let second_engine = Engine::new(EngineConfiguration::new());
		assert!(std::ptr::eq(Engine::get_engine().unwrap(), second_engine));
	}

	#[test]
	fn get_engine_inside_component_tick_from_function_without_context() {
		let engine = Engine::new(EngineConfiguration::new());
		let read_list = Rc::new(RefCell::new(Vec::new()));
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let reader_id = node_tree.create_child(root_id, "reader");
		node_tree[reader_id].add_component(GlobalReader {
			component_base: ComponentBase::new(),
			read_list: Rc::clone(&read_list),
		});
		node_tree.tick(engine, 0.016);
		node_tree.tick(engine, 0.016);
		let engine_address = get_engine_address(engine);
		assert_eq!(*read_list.borrow(), vec![Ok(engine_address), Ok(engine_address)]);
		// 빌려 준 자리가 따로 없다. 바깥에서도 꺼낸다.
		assert_eq!(read_global_engine_address(), Ok(engine_address));
	}

	struct RecordingScene {
		scene_base: SceneBase,
		read_list: Rc<RefCell<Vec<(&'static str, Result<usize, EngineAccessError>)>>>,
		promise: Option<Promise<()>>,
	}

	impl Scene for RecordingScene {
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}

		fn create(&mut self, engine: &Engine) {
			// 씬 함수의 engine 인자와 전역 엔진은 같은 엔진이다. 둘을 같이 써도 된다.
			self.read_list.borrow_mut().push(("create", read_global_engine_address()));
			assert_eq!(self.scene_base.get_engine().map(get_engine_address), Ok(get_engine_address(engine)));
			engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
			let read_list = Rc::clone(&self.read_list);
			let promise = self.promise.take().unwrap();
			engine.spawn_task(move |mut context| async move {
				read_list.borrow_mut().push(("task", read_global_engine_address()));
				promise.await;
				read_list.borrow_mut().push(("task after await", read_global_engine_address()));
				let inner_read = context.with(|_engine| read_global_engine_address());
				read_list.borrow_mut().push(("task with", inner_read));
			});
		}

		fn tick(&mut self, engine: &Engine, _time_delta: f64) {
			self.read_list.borrow_mut().push(("tick", read_global_engine_address()));
			assert!(std::ptr::eq(Engine::get_engine().unwrap(), engine));
		}
	}

	#[test]
	fn get_engine_inside_scene_functions_and_task() {
		let read_list = Rc::new(RefCell::new(Vec::new()));
		let (promise, promise_resolver) = Promise::create();
		let scene = RecordingScene {
			scene_base: SceneBase::new(),
			read_list: Rc::clone(&read_list),
			promise: Some(promise),
		};
		let engine = Engine::new(EngineConfiguration::new());
		let engine_address = get_engine_address(engine);
		let mut engine_runner = EngineRunner::new(engine, Box::new(scene));
		engine_runner.apply_pending_scenes();
		let mut scene = RecordingScene {
			scene_base: SceneBase::new(),
			read_list: Rc::clone(&read_list),
			promise: None,
		};
		scene.tick(engine, 0.016);
		promise_resolver.resolve(());
		task::poll_tasks(engine, &mut [], &mut []);
		let expected: Vec<(&'static str, Result<usize, EngineAccessError>)> = ["create", "task", "tick", "task after await", "task with"].iter().map(|name| (*name, Ok(engine_address))).collect();
		assert_eq!(*read_list.borrow(), expected);
	}

	#[test]
	#[should_panic(expected = "Engine::get_view_manager: 엔진의 뷰 매니저 칸을 이미 고치는 중이라 빌릴 수 없습니다")]
	fn overlapping_manager_borrow_panics_with_name() {
		let engine = Engine::new(EngineConfiguration::new());
		let _view_manager = engine.get_view_manager_mut();
		let _ = Engine::get_engine().unwrap().get_view_manager().get_view_size();
	}

	#[test]
	fn page_event_listener_gets_engine() {
		let engine = Engine::new(EngineConfiguration::new());
		let engine_address = get_engine_address(engine);
		let read = Rc::new(std::cell::Cell::new(None));
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
