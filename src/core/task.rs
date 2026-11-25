//==============================================================================
// 작업. (async/await 로 쓴 게임 코드를 엔진이 프레임마다 이어 돌린다. 러스트 판에만 있는 파일)
// - 원본은 브라우저가 Promise 를 돌리고, async 함수는 await 에서 멈췄다가 Promise 가 풀리면 이어 간다.
//   러스트 판은 엔진이 스레드 하나짜리 실행기를 품는다. Engine::spawn_task, spawn_scene_task 로 맡긴 퓨처를
//   프레임마다 한 번(시간과 입력을 갱신한 뒤, 씬의 tick 앞) 깨어난 것만 폴링한다. 외부 런타임은 쓰지 않고, 데스크탑과 웹이 같은 코드로 돈다.
//   퓨처는 Send 가 아니어도 된다. (Rc 를 들어도 된다, 웹은 스레드가 없다)
// - 깨우기: 작업마다 깨움 표시(Arc 에 든 AtomicBool)를 std::task::Wake 로 Waker 로 만든다. 깨우면 표시만 세우고,
//   실행기는 폴링 차례에 목록을 한 번 돌며 표시가 선 작업만 표시를 내리고 폴링한다. 그래서 한 작업은 한 프레임에 많아야 한 번 폴링하고,
//   폴링하는 동안 스스로 깨운 작업은 다음 프레임에 다시 폴링한다. (앞의 작업이 깨운 뒤쪽 작업은 같은 차례에 폴링한다)
//   엔진의 상태를 보는 퓨처(애셋 손잡이, wait::seconds, wait::next_frame 같은 것)는 아직이면 스스로 깨워 다음 프레임에 다시 본다.
//   Promise 는 resolve 가 깨운다. 웹 Promise 를 감싼 JsFuture 는 브라우저가 풀 때 깨운다. (Waker 는 Send + Sync 라 Arc 와 원자 값으로 든다)
// - 엔진과 씬에 닿기: 퓨처는 &mut 를 await 너머로 쥘 수 없다. 작업은 컨텍스트(EngineContext, SceneContext)를 받고,
//   with(|씬, 엔진| ...) 클로저 안에서만 엔진과 씬을 빌린다. 클로저는 동기라 그 안에서 await 할 수 없고, 클로저가 받는 참조의 수명은
//   클로저마다 새로 매겨져(for<'a>) 돌려주는 값(R)에 담을 수 없으니 참조가 클로저 밖으로 새지 못한다.
//   컨텍스트는 복제할 수 없고 with 가 &mut self 를 받아서, 한 작업 안에서 with 를 겹쳐 부르는 코드는 컴파일되지 않는다.
// - 건전성: 폴링하는 동안만 스레드 지역 칸(TASK_ACCESS)에 엔진, 그래픽, 그 작업의 씬을 가리키는 포인터를 둔다.
//   1. 포인터는 실행기가 &mut 로 쥔 엔진, 그래픽, 씬 목록의 한 칸에서 만든다. 폴링이 끝날 때까지 실행기는 그 &mut 를 다시 쓰지 않고,
//      폴링이 끝나면(패닉으로 풀릴 때도) 칸을 비운다. 그래서 포인터는 쓰는 동안 늘 살아 있고, 실행기 쪽 빌림과 겹치지 않는다.
//   2. 엔진(EngineRunner.engine), 그래픽(EngineRunner.graphic), 씬(EngineRunner.scene_list 의 한 칸)은 서로 다른 값이라 겹치지 않는다.
//      폴링하는 작업 목록은 엔진에서 꺼내 둔 지역 값이라 엔진을 빌려도 닿지 않는다. (그동안 맡긴 새 작업은 엔진의 따로 둔 목록에 쌓인다)
//   3. with 는 칸에서 포인터를 꺼내(take) 쓰고 끝나면 되돌린다. 그동안 칸이 비어 있어서, 어떤 길로든 두 번째 &mut 를 만들 수 없다.
//      (작업 바깥에서 부르거나, 안에서 겹쳐 부르거나, 다른 씬의 컨텍스트를 몰래 넘겨 부르면 이중 빌림 대신 까닭을 적은 패닉이 난다)
//   4. 씬은 Any 로 그 컨텍스트의 타입인지 확인하고, 씬 핸들이 그 작업의 씬인지도 확인한다.
// - 씬 작업은 그 씬과 수명을 같이한다. 씬을 내리면 그 씬의 작업도 버린다. (원본은 씬을 내려도 Promise 사슬이 이어 돈다)
// - Promise: 원본 new Promise((resolve) => ...) 자리다. Promise::create 가 퓨처와 resolve 손잡이를 짝으로 준다.
//   손잡이를 버리고 resolve 하지 않으면 원본처럼 끝나지 않는다.
//==============================================================================
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::scene::{Scene, SceneId};
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

//==============================================================================
// 작업을 깨우는 표시. (Waker 는 Send + Sync 라서 Arc 와 원자 값으로 든다. 웹은 스레드가 없어도 같은 코드다)
//==============================================================================
struct TaskWakeFlag {
	is_woken: AtomicBool,
}

impl Wake for TaskWakeFlag {
	//==============================================================================
	// 깨우기. (표시만 세운다, 다음 폴링 차례에 폴링한다)
	//==============================================================================
	fn wake(self: Arc<TaskWakeFlag>) {
		self.is_woken.store(true, Ordering::Release);
	}

	//==============================================================================
	// 깨우기. (빌린 것으로)
	//==============================================================================
	fn wake_by_ref(self: &Arc<TaskWakeFlag>) {
		self.is_woken.store(true, Ordering::Release);
	}
}

//==============================================================================
// 작업의 끝 상태. (작업과 손잡이가 나눠 든다)
//==============================================================================
#[derive(Debug, Default)]
struct TaskState {
	is_finished: Cell<bool>,
}

//==============================================================================
// 작업 손잡이. (spawn_task, spawn_scene_task 가 돌려준다. 원본 async 함수가 돌려주는 Promise 자리)
// - 끝났는지 묻거나(is_finished), 다른 작업에서 await 하거나, 씬이 track_task 로 맡겨 다 끝날 때까지 initialize 를 미룬다.
// - 작업을 버렸을 때(그 씬을 내렸을 때)도 끝난 것으로 친다.
//==============================================================================
#[derive(Clone, Debug)]
pub struct TaskHandle {
	task_state: Rc<TaskState>,
}

impl TaskHandle {
	//==============================================================================
	// 끝났는지. (다 돌았거나 버려졌거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.task_state.is_finished.get()
	}
}

impl Future for TaskHandle {
	type Output = ();

	//==============================================================================
	// 끝날 때까지 기다리기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut TaskHandle>, context: &mut Context<'_>) -> Poll<()> {
		if self.is_finished() {
			return Poll::Ready(());
		}
		context.waker().wake_by_ref();
		Poll::Pending
	}
}

//==============================================================================
// 작업 하나. (퓨처와 깨움 표시, 씬 작업이면 그 씬의 핸들)
//==============================================================================
struct Task {
	scene_id: Option<SceneId>,
	future: Pin<Box<dyn Future<Output = ()>>>,
	wake_flag: Arc<TaskWakeFlag>,
	waker: Waker,
	task_state: Rc<TaskState>,
	is_done: bool,
}

impl Drop for Task {
	//==============================================================================
	// 버리기. (다 돌았든 버려졌든 손잡이에는 끝난 것으로 알린다)
	//==============================================================================
	fn drop(&mut self) {
		self.task_state.is_finished.set(true);
	}
}

//==============================================================================
// 실행기. (엔진이 들고, EngineRunner 가 프레임마다 poll_tasks 로 돌린다)
//==============================================================================
pub(crate) struct TaskExecutor {
	task_list: Vec<Task>,
	// 맡긴 새 작업. (폴링하는 동안에도 엔진을 &self 로 빌린 곳에서 맡길 수 있게 따로 둔다)
	spawned_task_list: RefCell<Vec<Task>>,
}

impl TaskExecutor {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub(crate) fn new() -> TaskExecutor {
		TaskExecutor {
			task_list: Vec::new(),
			spawned_task_list: RefCell::new(Vec::new()),
		}
	}

	//==============================================================================
	// 작업 맡기기. (깨어난 채로 넣어 다음 폴링 차례에 처음 폴링한다)
	//==============================================================================
	pub(crate) fn spawn(&self, scene_id: Option<SceneId>, future: Pin<Box<dyn Future<Output = ()>>>) -> TaskHandle {
		let wake_flag = Arc::new(TaskWakeFlag {
			is_woken: AtomicBool::new(true),
		});
		let waker = Waker::from(wake_flag.clone());
		let task_state = Rc::new(TaskState::default());
		self.spawned_task_list.borrow_mut().push(Task {
			scene_id,
			future,
			wake_flag,
			waker,
			task_state: task_state.clone(),
			is_done: false,
		});
		TaskHandle {
			task_state,
		}
	}

	//==============================================================================
	// 씬의 작업 버리기. (그 씬을 내릴 때 엔진이 부른다)
	//==============================================================================
	pub(crate) fn cancel_scene_tasks(&mut self, scene_id: SceneId) {
		self.task_list.retain(|task| task.scene_id != Some(scene_id));
		self.spawned_task_list.get_mut().retain(|task| task.scene_id != Some(scene_id));
	}

	//==============================================================================
	// 남은 작업 수. (맡긴 새 작업 포함, 시험이 본다)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn get_task_count(&self) -> usize {
		self.task_list.len() + self.spawned_task_list.borrow().len()
	}
}

//==============================================================================
// 폴링하는 동안 작업이 닿는 것. (엔진, 그래픽, 그 작업의 씬을 가리키는 포인터)
//==============================================================================
#[derive(Clone, Copy)]
struct TaskAccess {
	engine: NonNull<Engine>,
	graphic: Option<NonNull<Graphic>>,
	scene: Option<NonNull<dyn Scene>>,
	scene_id: Option<SceneId>,
}

thread_local! {
	// 지금 폴링하는 작업이 닿는 것. (폴링하는 동안만 Some, with 가 쓰는 동안은 꺼내 두어 None)
	static TASK_ACCESS: Cell<Option<TaskAccess>> = const { Cell::new(None) };
}

//==============================================================================
// 폴링하는 동안 칸에 둔 것을 지키는 값. (버릴 때 앞의 값으로 되돌린다. 패닉으로 풀릴 때도)
//==============================================================================
struct TaskAccessScope {
	previous_access: Option<TaskAccess>,
}

impl TaskAccessScope {
	//==============================================================================
	// 칸에 두기.
	//==============================================================================
	fn install(task_access: TaskAccess) -> TaskAccessScope {
		let previous_access = TASK_ACCESS.with(|access_cell| access_cell.replace(Some(task_access)));
		TaskAccessScope {
			previous_access,
		}
	}
}

impl Drop for TaskAccessScope {
	//==============================================================================
	// 되돌리기.
	//==============================================================================
	fn drop(&mut self) {
		TASK_ACCESS.with(|access_cell| access_cell.set(self.previous_access));
	}
}

//==============================================================================
// 칸에서 꺼내 쓰는 동안의 값. (꺼내 두어 겹쳐 빌릴 수 없게 하고, 버릴 때 칸에 되돌린다. 패닉으로 풀릴 때도)
//==============================================================================
struct TaskAccessLease {
	task_access: TaskAccess,
}

impl TaskAccessLease {
	//==============================================================================
	// 꺼내기. (폴링하는 작업 안이 아니거나 이미 꺼내 쓰는 중이면 None)
	//==============================================================================
	fn try_take() -> Option<TaskAccessLease> {
		let task_access = TASK_ACCESS.with(|access_cell| access_cell.take())?;
		Some(TaskAccessLease {
			task_access,
		})
	}

	//==============================================================================
	// 꺼내기. (못 꺼내면 까닭을 적고 패닉한다)
	//==============================================================================
	#[track_caller]
	fn take(function_name: &str) -> TaskAccessLease {
		match TaskAccessLease::try_take() {
			Some(task_access_lease) => {
				task_access_lease
			}
			None => {
				panic!("{}: 엔진이 작업을 폴링하는 동안, with 바깥에서만 부를 수 있습니다", function_name);
			}
		}
	}

	//==============================================================================
	// 엔진 빌리기.
	//==============================================================================
	fn get_engine_mut(&mut self) -> &mut Engine {
		// SAFETY: 포인터는 poll_tasks 가 &mut Engine 에서 만들었고, 칸에 있는 동안 poll_tasks 는 그 &mut 를 쓰지 않는다.
		// 칸에서 꺼내 둔 동안은 이 값 말고는 엔진에 닿을 길이 없고, 돌려주는 참조는 &mut self 에 묶여 이 값보다 오래 살지 못한다.
		unsafe { &mut *self.task_access.engine.as_ptr() }
	}

	//==============================================================================
	// 그 작업의 씬과 엔진 빌리기. (scene_id 의 씬이 아니면 None)
	//==============================================================================
	fn get_scene_and_engine_mut(&mut self, scene_id: SceneId) -> Option<(&mut dyn Scene, &mut Engine, Option<&mut Graphic>)> {
		if self.task_access.scene_id != Some(scene_id) {
			return None;
		}
		let scene_pointer = self.task_access.scene?;
		// SAFETY: 셋 모두 poll_tasks 가 &mut 에서 만든 포인터이고 서로 다른 값(씬 목록의 한 칸, 엔진, 그래픽)을 가리킨다.
		// 칸에서 꺼내 둔 동안은 이 값 말고는 닿을 길이 없고, 돌려주는 참조는 &mut self 에 묶여 이 값보다 오래 살지 못한다.
		let scene = unsafe { &mut *scene_pointer.as_ptr() };
		let engine = unsafe { &mut *self.task_access.engine.as_ptr() };
		let graphic = self.task_access.graphic.map(|graphic_pointer| unsafe { &mut *graphic_pointer.as_ptr() });
		Some((scene, engine, graphic))
	}

	//==============================================================================
	// 엔진과 그래픽 빌리기. (그래픽이 없으면 None)
	//==============================================================================
	fn get_engine_and_graphic_mut(&mut self) -> Option<(&mut Engine, &mut Graphic)> {
		let graphic_pointer = self.task_access.graphic?;
		// SAFETY: get_engine_mut 와 같다. 엔진과 그래픽은 EngineRunner 의 서로 다른 칸이다.
		let engine = unsafe { &mut *self.task_access.engine.as_ptr() };
		let graphic = unsafe { &mut *graphic_pointer.as_ptr() };
		Some((engine, graphic))
	}
}

impl Drop for TaskAccessLease {
	//==============================================================================
	// 칸에 되돌리기.
	//==============================================================================
	fn drop(&mut self) {
		TASK_ACCESS.with(|access_cell| access_cell.set(Some(self.task_access)));
	}
}

//==============================================================================
// 폴링하는 작업 안에서 엔진 빌리기. (엔진 상태를 보며 이어 가는 퓨처가 쓴다. 작업 바깥이거나 with 안이면 None)
//==============================================================================
pub(crate) fn with_polling_engine<R>(engine_function: impl FnOnce(&mut Engine) -> R) -> Option<R> {
	let mut task_access_lease = TaskAccessLease::try_take()?;
	let result = engine_function(task_access_lease.get_engine_mut());
	drop(task_access_lease);
	Some(result)
}

//==============================================================================
// 폴링하는 작업 안에서 그래픽 빌리기. (GL 이 있어야 끝나는 퓨처가 쓴다. 작업 바깥이거나 with 안이거나 그래픽이 없으면 None)
//==============================================================================
pub(crate) fn with_polling_graphic<R>(graphic_function: impl FnOnce(&mut Graphic) -> R) -> Option<R> {
	let mut task_access_lease = TaskAccessLease::try_take()?;
	let result = task_access_lease.get_engine_and_graphic_mut().map(|(_, graphic)| graphic_function(graphic));
	drop(task_access_lease);
	result
}

//==============================================================================
// 엔진 작업의 컨텍스트. (Engine::spawn_task 가 작업 함수에 넘긴다. with 안에서만 엔진을 빌린다)
// - 복제할 수 없고 스레드를 건너지 않는다.
//==============================================================================
pub struct EngineContext {
	_not_send: PhantomData<*const ()>,
}

impl EngineContext {
	//==============================================================================
	// 생성. (실행기가 작업마다 하나 만든다)
	//==============================================================================
	fn new() -> EngineContext {
		EngineContext {
			_not_send: PhantomData,
		}
	}

	//==============================================================================
	// 엔진 빌리기. (클로저가 도는 동안만, 클로저 안에서는 await 할 수 없다)
	// - 엔진이 이 작업을 폴링하는 동안에만 부를 수 있다. (다른 곳에서 부르면 패닉)
	//==============================================================================
	#[track_caller]
	pub fn with<R>(&mut self, engine_function: impl FnOnce(&mut Engine) -> R) -> R {
		let mut task_access_lease = TaskAccessLease::take("EngineContext::with");
		let result = engine_function(task_access_lease.get_engine_mut());
		drop(task_access_lease);
		result
	}

	//==============================================================================
	// 엔진과 그래픽 빌리기. (GL 이 있어야 하는 일, 그래픽을 붙이기 전이면 None)
	//==============================================================================
	#[track_caller]
	pub fn with_graphic<R>(&mut self, graphic_function: impl FnOnce(&mut Engine, &mut Graphic) -> R) -> Option<R> {
		let mut task_access_lease = TaskAccessLease::take("EngineContext::with_graphic");
		let result = task_access_lease.get_engine_and_graphic_mut().map(|(engine, graphic)| graphic_function(engine, graphic));
		drop(task_access_lease);
		result
	}
}

//==============================================================================
// 씬 작업의 컨텍스트. (Engine::spawn_scene_task 가 작업 함수에 넘긴다. with 안에서만 그 씬과 엔진을 빌린다)
// - 씬은 S 타입 그대로 받는다. (원본 async 함수 안의 this 자리)
// - 복제할 수 없고 스레드를 건너지 않는다.
//==============================================================================
pub struct SceneContext<S: Scene + 'static> {
	scene_id: SceneId,
	_scene_type: PhantomData<(*const (), fn() -> S)>,
}

impl<S: Scene + 'static> SceneContext<S> {
	//==============================================================================
	// 생성. (실행기가 작업마다 하나 만든다)
	//==============================================================================
	fn new(scene_id: SceneId) -> SceneContext<S> {
		SceneContext {
			scene_id,
			_scene_type: PhantomData,
		}
	}

	//==============================================================================
	// 이 작업의 씬 핸들.
	//==============================================================================
	pub fn get_scene_id(&self) -> SceneId {
		self.scene_id
	}

	//==============================================================================
	// 씬과 엔진 빌리기. (클로저가 도는 동안만, 클로저 안에서는 await 할 수 없다)
	// - 엔진이 이 작업을 폴링하는 동안에만 부를 수 있다. (다른 곳에서 부르면 패닉)
	//==============================================================================
	#[track_caller]
	pub fn with<R>(&mut self, scene_function: impl FnOnce(&mut S, &mut Engine) -> R) -> R {
		let mut task_access_lease = TaskAccessLease::take("SceneContext::with");
		let (scene, engine, _) = match task_access_lease.get_scene_and_engine_mut(self.scene_id) {
			Some(scene_and_engine) => {
				scene_and_engine
			}
			None => {
				panic!("SceneContext::with: 이 씬의 작업 안에서만 부를 수 있습니다");
			}
		};
		let scene = match scene.as_scene_any_mut().downcast_mut::<S>() {
			Some(scene) => {
				scene
			}
			None => {
				panic!("SceneContext::with: 씬이 컨텍스트의 타입이 아닙니다 (씬 공통 상태를 다른 씬에서 빌려 쓴 경우)");
			}
		};
		let result = scene_function(scene, engine);
		drop(task_access_lease);
		result
	}

	//==============================================================================
	// 씬과 엔진과 그래픽 빌리기. (GL 이 있어야 하는 일, 그래픽을 붙이기 전이면 None)
	//==============================================================================
	#[track_caller]
	pub fn with_graphic<R>(&mut self, graphic_function: impl FnOnce(&mut S, &mut Engine, &mut Graphic) -> R) -> Option<R> {
		let mut task_access_lease = TaskAccessLease::take("SceneContext::with_graphic");
		let (scene, engine, graphic) = match task_access_lease.get_scene_and_engine_mut(self.scene_id) {
			Some(scene_and_engine) => {
				scene_and_engine
			}
			None => {
				panic!("SceneContext::with_graphic: 이 씬의 작업 안에서만 부를 수 있습니다");
			}
		};
		let scene = match scene.as_scene_any_mut().downcast_mut::<S>() {
			Some(scene) => {
				scene
			}
			None => {
				panic!("SceneContext::with_graphic: 씬이 컨텍스트의 타입이 아닙니다");
			}
		};
		let result = graphic.map(|graphic| graphic_function(scene, engine, graphic));
		drop(task_access_lease);
		result
	}
}

//==============================================================================
// 엔진 작업 맡기기. (Engine::spawn_task 가 부른다)
//==============================================================================
pub(crate) fn spawn_engine_task<F, TaskFuture>(task_executor: &TaskExecutor, task_function: F) -> TaskHandle
where
	F: FnOnce(EngineContext) -> TaskFuture,
	TaskFuture: Future<Output = ()> + 'static,
{
	let future = task_function(EngineContext::new());
	task_executor.spawn(None, Box::pin(future))
}

//==============================================================================
// 씬 작업 맡기기. (Engine::spawn_scene_task 가 부른다)
//==============================================================================
pub(crate) fn spawn_scene_task<S, F, TaskFuture>(task_executor: &TaskExecutor, scene_id: SceneId, task_function: F) -> TaskHandle
where
	S: Scene + 'static,
	F: FnOnce(SceneContext<S>) -> TaskFuture,
	TaskFuture: Future<Output = ()> + 'static,
{
	let future = task_function(SceneContext::<S>::new(scene_id));
	task_executor.spawn(Some(scene_id), Box::pin(future))
}

//==============================================================================
// 깨어난 작업 폴링하기. (EngineRunner 가 프레임마다 한 번, 씬의 tick 앞에 부른다)
// - 작업 목록을 엔진에서 꺼내 돌리고, 그동안 맡긴 새 작업은 뒤에 붙인다. (다음 프레임에 처음 폴링한다)
// - 씬 작업의 씬이 목록에 없으면(내렸으면) 작업을 버린다.
//==============================================================================
pub(crate) fn poll_tasks(engine: &mut Engine, scene_list: &mut [Box<dyn Scene>], mut graphic: Option<&mut Graphic>) {
	let mut task_list = std::mem::take(&mut engine.task_executor.task_list);
	task_list.append(engine.task_executor.spawned_task_list.get_mut());
	for task in task_list.iter_mut() {
		if !task.wake_flag.is_woken.swap(false, Ordering::AcqRel) {
			continue;
		}
		let scene_pointer = match task.scene_id {
			Some(scene_id) => {
				match scene_list.iter_mut().find(|scene| scene.get_scene_base().get_scene_id() == Some(scene_id)) {
					Some(scene) => {
						Some(NonNull::from(&mut **scene))
					}
					None => {
						task.is_done = true;
						continue;
					}
				}
			}
			None => {
				None
			}
		};
		let task_access = TaskAccess {
			engine: NonNull::from(&mut *engine),
			graphic: graphic.as_deref_mut().map(NonNull::from),
			scene: scene_pointer,
			scene_id: task.scene_id,
		};
		let poll_result = {
			let _task_access_scope = TaskAccessScope::install(task_access);
			let mut context = Context::from_waker(&task.waker);
			task.future.as_mut().poll(&mut context)
		};
		if poll_result.is_ready() {
			task.is_done = true;
		}
	}
	task_list.retain(|task| !task.is_done);
	task_list.append(engine.task_executor.spawned_task_list.get_mut());
	engine.task_executor.task_list = task_list;
}

//==============================================================================
// Promise 의 안쪽. (값과 기다리는 작업의 Waker)
//==============================================================================
struct PromiseState<T> {
	value: Option<T>,
	waker: Option<Waker>,
}

//==============================================================================
// Promise. (원본 new Promise((resolve) => ...) 자리, PromiseResolver::resolve 로 풀리면 그 값으로 끝나는 퓨처)
// - 콜백으로 끝을 알리는 API 를 await 할 수 있게 바꿀 때 쓴다. (read_clipboard_text_async, request_frame_capture_async 같은 것)
// - 손잡이를 버리고 resolve 하지 않으면 원본처럼 끝나지 않는다.
//==============================================================================
pub struct Promise<T> {
	promise_state: Rc<RefCell<PromiseState<T>>>,
}

//==============================================================================
// Promise 를 푸는 손잡이. (원본 resolve 함수 자리, 한 번만 풀 수 있게 resolve 가 손잡이를 가져간다)
//==============================================================================
pub struct PromiseResolver<T> {
	promise_state: Rc<RefCell<PromiseState<T>>>,
}

impl<T> Promise<T> {
	//==============================================================================
	// 생성. (퓨처와 푸는 손잡이를 짝으로 준다)
	//==============================================================================
	pub fn create() -> (Promise<T>, PromiseResolver<T>) {
		let promise_state = Rc::new(RefCell::new(PromiseState {
			value: None,
			waker: None,
		}));
		let promise = Promise {
			promise_state: promise_state.clone(),
		};
		let promise_resolver = PromiseResolver {
			promise_state,
		};
		(promise, promise_resolver)
	}

	//==============================================================================
	// 이미 풀린 Promise. (원본 Promise.resolve(값))
	//==============================================================================
	pub fn resolved(value: T) -> Promise<T> {
		let (promise, promise_resolver) = Promise::create();
		promise_resolver.resolve(value);
		promise
	}
}

impl<T> PromiseResolver<T> {
	//==============================================================================
	// 풀기. (기다리는 작업을 깨운다. 빌림을 놓은 뒤 깨워서, 깨우는 쪽이 바로 폴링해도 겹쳐 빌리지 않는다)
	//==============================================================================
	pub fn resolve(self, value: T) {
		let waker = {
			let mut promise_state = self.promise_state.borrow_mut();
			promise_state.value = Some(value);
			promise_state.waker.take()
		};
		if let Some(waker) = waker {
			waker.wake();
		}
	}
}

impl<T> Future for Promise<T> {
	type Output = T;

	//==============================================================================
	// 풀릴 때까지 기다리기. (풀리면 그 값, 아니면 깨울 Waker 를 적어 둔다)
	//==============================================================================
	fn poll(self: Pin<&mut Promise<T>>, context: &mut Context<'_>) -> Poll<T> {
		let mut promise_state = self.promise_state.borrow_mut();
		if let Some(value) = promise_state.value.take() {
			return Poll::Ready(value);
		}
		promise_state.waker = Some(context.waker().clone());
		Poll::Pending
	}
}

impl<T> std::fmt::Debug for Promise<T> {
	//==============================================================================
	// 디버그 출력. (풀렸는지만)
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("Promise").field("is_resolved", &self.promise_state.borrow().value.is_some()).finish()
	}
}

impl<T> std::fmt::Debug for PromiseResolver<T> {
	//==============================================================================
	// 디버그 출력.
	//==============================================================================
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		formatter.debug_struct("PromiseResolver").finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::wait;
	use crate::core::asset::{Asset, AssetLoadHandle};
	use crate::core::engine::EngineConfiguration;
	use crate::core::scene::SceneBase;
	use crate::resource::imageasset::ImageAsset;

	//==============================================================================
	// 폴링한 수를 세는 퓨처. (깨우지 않은 작업을 실행기가 건드리지 않는지 본다)
	//==============================================================================
	struct PollCountFuture<F> {
		future: Pin<Box<F>>,
		poll_count: Rc<Cell<u32>>,
	}

	impl<F: Future> Future for PollCountFuture<F> {
		type Output = F::Output;

		//==============================================================================
		// 세고 안쪽 퓨처 폴링.
		//==============================================================================
		fn poll(self: Pin<&mut PollCountFuture<F>>, context: &mut Context<'_>) -> Poll<F::Output> {
			let poll_count_future = self.get_mut();
			poll_count_future.poll_count.set(poll_count_future.poll_count.get() + 1);
			poll_count_future.future.as_mut().poll(context)
		}
	}

	//==============================================================================
	// 시험용 씬. (작업이 with 로 바꾸는 값)
	//==============================================================================
	struct CounterScene {
		scene_base: SceneBase,
		value: u32,
	}

	impl Scene for CounterScene {
		fn get_scene_base(&self) -> &SceneBase {
			&self.scene_base
		}

		fn get_scene_base_mut(&mut self) -> &mut SceneBase {
			&mut self.scene_base
		}
	}

	//==============================================================================
	// 씬 목록의 시험용 씬 값 읽기.
	//==============================================================================
	fn read_counter_value(scene_list: &mut [Box<dyn Scene>]) -> u32 {
		scene_list[0].as_scene_any_mut().downcast_mut::<CounterScene>().expect("시험용 씬").value
	}

	//==============================================================================
	// 실행기: 맡긴 작업은 다음 폴링 차례에 처음 돌고, 깨어난 작업은 한 차례에 한 번만 폴링한다. 끝나면 손잡이가 끝난 것이 된다.
	//==============================================================================
	#[test]
	fn executor_polls_woken_tasks_once_per_frame() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let step_count = Rc::new(Cell::new(0));
		let task_step_count = step_count.clone();
		let task_handle = engine.spawn_task(move |_engine_context| async move {
			for _ in 0..3 {
				task_step_count.set(task_step_count.get() + 1);
				wait::next_frame().await;
			}
		});
		assert_eq!(step_count.get(), 0);
		assert_eq!(engine.task_executor.get_task_count(), 1);
		for expected_step_count in 1..=3 {
			poll_tasks(&mut engine, &mut scene_list, None);
			assert_eq!(step_count.get(), expected_step_count);
			assert!(!task_handle.is_finished());
		}
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(task_handle.is_finished());
		assert_eq!(engine.task_executor.get_task_count(), 0);
	}

	//==============================================================================
	// 깨우기: Promise 를 기다리는 작업은 풀릴 때까지 폴링하지 않고, resolve 가 깨우면 다음 차례에 그 값으로 이어 간다. 작업은 다른 작업을 await 한다.
	//==============================================================================
	#[test]
	fn promise_wakes_task_only_when_resolved() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let (value_promise, value_resolver) = Promise::<u32>::create();
		let poll_count = Rc::new(Cell::new(0));
		let received_value = Rc::new(Cell::new(0));
		let task_received_value = received_value.clone();
		let task_poll_count = poll_count.clone();
		let waiting_task_handle = engine.spawn_task(move |_engine_context| PollCountFuture {
			future: Box::pin(async move {
				task_received_value.set(value_promise.await);
			}),
			poll_count: task_poll_count,
		});
		let joined = Rc::new(Cell::new(false));
		let task_joined = joined.clone();
		let joined_waiting_task_handle = waiting_task_handle.clone();
		engine.spawn_task(move |_engine_context| async move {
			joined_waiting_task_handle.await;
			task_joined.set(true);
		});
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(poll_count.get(), 1);
		for _ in 0..3 {
			poll_tasks(&mut engine, &mut scene_list, None);
		}
		assert_eq!(poll_count.get(), 1);
		assert!(!waiting_task_handle.is_finished());
		value_resolver.resolve(7);
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(poll_count.get(), 2);
		assert_eq!(received_value.get(), 7);
		assert!(waiting_task_handle.is_finished());
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(joined.get());
		assert!(Promise::resolved(3).promise_state.borrow().value.is_some());
	}

	//==============================================================================
	// 기다리기: wait::seconds 는 원본 setTimeout 처럼 벽시계 초라 시간 배율이 0 이어도 흐른다. 음수와 NaN 은 0 초, next_frame 은 다음 차례.
	//==============================================================================
	#[test]
	fn wait_seconds_follows_wall_clock() {
		let mut engine = Engine::new(EngineConfiguration::new());
		engine.get_time_manager_mut().set_time_scale(0.0);
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let progress = Rc::new(Cell::new(0));
		let task_progress = progress.clone();
		let task_handle = engine.spawn_task(move |_engine_context| async move {
			wait::seconds(-1.0).await;
			wait::seconds(f32::NAN).await;
			task_progress.set(1);
			wait::seconds(0.03).await;
			task_progress.set(2);
		});
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(progress.get(), 1);
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(progress.get(), 1);
		std::thread::sleep(std::time::Duration::from_millis(45));
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(progress.get(), 2);
		assert!(task_handle.is_finished());
	}

	//==============================================================================
	// 애셋 퓨처: 손잡이를 그대로 await 하면 다 읽힌 바이트나 실패 까닭이 나오고, 요청 전인 애셋의 wait_for_load 는 바로 실패다.
	//==============================================================================
	#[test]
	fn asset_futures_finish_with_load_handles() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let load_handle = AssetLoadHandle::create_pending("data.bin");
		let failed_handle = AssetLoadHandle::create_pending("missing.bin");
		let mut image_asset = ImageAsset::new();
		let result_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let task_result_list = result_list.clone();
		let task_load_handle = load_handle.clone();
		let task_failed_handle = failed_handle.clone();
		let not_requested_future = image_asset.wait_for_load();
		image_asset.load_from_bytes("memory.png", vec![1, 2]);
		let loaded_future = image_asset.wait_for_load();
		engine.spawn_task(move |_engine_context| async move {
			let bytes = task_load_handle.await.expect("읽힘");
			task_result_list.borrow_mut().push(format!("bytes {}", bytes.len()));
			let error_message = (&task_failed_handle).await.expect_err("실패");
			task_result_list.borrow_mut().push(error_message);
			task_result_list.borrow_mut().push(format!("not requested {}", not_requested_future.await.is_err()));
			task_result_list.borrow_mut().push(format!("memory {}", loaded_future.await.expect("바이트").len()));
		});
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(result_list.borrow().is_empty());
		load_handle.finish(Ok(vec![1, 2, 3]));
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(*result_list.borrow(), vec![String::from("bytes 3")]);
		failed_handle.finish(Err(String::from("없음")));
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(*result_list.borrow(), vec![String::from("bytes 3"), String::from("없음"), String::from("not requested true"), String::from("memory 2")]);
	}

	//==============================================================================
	// 엔진을 빌려 이어 가는 퓨처: AssetLoader::wait_for_load 는 기다리는 동안 update 를 불러 실패 목록으로 끝나고,
	// Timeline::load_from_url 은 받지 못하면 까닭으로 끝난다.
	//==============================================================================
	#[test]
	fn engine_driven_futures_finish() {
		use crate::animation::timeline::Timeline;
		use crate::resource::assetloader::AssetLoader;
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let asset_loader = AssetLoader::new();
		let external_handle = AssetLoadHandle::create_pending("stage");
		asset_loader.add_load_handle(&external_handle);
		asset_loader.load(&engine);
		let timeline_future = Timeline::load_from_url(&engine, "missing-timeline.json");
		let result_list: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
		let task_result_list = result_list.clone();
		let task_asset_loader = asset_loader.clone();
		engine.spawn_task(move |_engine_context| async move {
			let failed_list = task_asset_loader.wait_for_load().await;
			task_result_list.borrow_mut().push(format!("loader {}", failed_list.len()));
			task_result_list.borrow_mut().push(format!("timeline {}", timeline_future.await.is_err()));
		});
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(result_list.borrow().is_empty());
		assert!(!asset_loader.is_finished());
		external_handle.finish(Ok(Vec::new()));
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(*result_list.borrow(), vec![String::from("loader 0"), String::from("timeline true")]);
		assert_eq!(asset_loader.get_finished_count(), 1);
	}

	//==============================================================================
	// 컨텍스트: 씬 작업은 with 안에서 그 씬(제 타입)과 엔진을 빌리고, await 다음 줄에서도 다시 빌린다. 그 안에서 새 작업을 맡겨도 된다.
	// 씬이 목록에 없으면(내렸으면) 작업을 버리고 손잡이는 끝난 것이 된다.
	//==============================================================================
	#[test]
	fn scene_context_reaches_scene_and_engine_between_awaits() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene = CounterScene {
			scene_base: SceneBase::new(),
			value: 0,
		};
		scene.scene_base.set_scene_id(SceneId(3));
		let child_finished = Rc::new(Cell::new(false));
		let task_child_finished = child_finished.clone();
		let task_handle = engine.spawn_scene_task(&scene, move |mut scene_context| async move {
			let frame_number = scene_context.with(|scene, engine| {
				scene.value += 1;
				engine.get_frame_number()
			});
			assert_eq!(frame_number, 1);
			wait::next_frame().await;
			scene_context.with(|scene, engine| {
				scene.value += 10;
				engine.spawn_task(move |mut engine_context| async move {
					engine_context.with(|engine| {
						engine.get_time_manager_mut().set_time_scale(0.5);
					});
					task_child_finished.set(true);
				});
			});
			assert!(scene_context.with_graphic(|_scene, _engine, _graphic| ()).is_none());
		});
		let mut scene_list: Vec<Box<dyn Scene>> = vec![Box::new(scene)];
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(read_counter_value(&mut scene_list), 1);
		poll_tasks(&mut engine, &mut scene_list, None);
		assert_eq!(read_counter_value(&mut scene_list), 11);
		assert!(task_handle.is_finished());
		assert!(!child_finished.get());
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(child_finished.get());
		assert_eq!(engine.get_time_manager().get_time_scale(), 0.5);

		let orphan_task_handle = engine.spawn_scene_task(&CounterScene {
			scene_base: SceneBase::new(),
			value: 0,
		}, |_scene_context| async {});
		poll_tasks(&mut engine, &mut scene_list, None);
		assert!(orphan_task_handle.is_finished());
		assert_eq!(engine.task_executor.get_task_count(), 0);
	}

	//==============================================================================
	// 건전성: 작업 바깥으로 빼돌린 컨텍스트로 with 를 부르면 이중 빌림 대신 까닭을 적은 패닉이 난다.
	//==============================================================================
	#[test]
	#[should_panic(expected = "엔진이 작업을 폴링하는 동안")]
	fn context_outside_polling_panics() {
		let engine = Engine::new(EngineConfiguration::new());
		let smuggled_context: Rc<RefCell<Option<EngineContext>>> = Rc::new(RefCell::new(None));
		let task_smuggled_context = smuggled_context.clone();
		engine.spawn_task(move |engine_context| {
			*task_smuggled_context.borrow_mut() = Some(engine_context);
			async {}
		});
		let mut engine_context = smuggled_context.borrow_mut().take().expect("컨텍스트");
		engine_context.with(|_engine| ());
	}

	//==============================================================================
	// 건전성: with 안에서 다른 작업의 컨텍스트로 with 를 겹쳐 부르면 칸이 비어 있어 두 번째 &mut 를 만들지 못하고 패닉한다.
	//==============================================================================
	#[test]
	#[should_panic(expected = "with 바깥에서만")]
	fn nested_with_through_other_context_panics() {
		let mut engine = Engine::new(EngineConfiguration::new());
		let mut scene_list: Vec<Box<dyn Scene>> = Vec::new();
		let smuggled_context: Rc<RefCell<Option<EngineContext>>> = Rc::new(RefCell::new(None));
		let task_smuggled_context = smuggled_context.clone();
		engine.spawn_task(move |engine_context| {
			*task_smuggled_context.borrow_mut() = Some(engine_context);
			async {}
		});
		let nesting_smuggled_context = smuggled_context.clone();
		engine.spawn_task(move |mut engine_context| async move {
			engine_context.with(|_engine| {
				let mut other_context = nesting_smuggled_context.borrow_mut().take().expect("컨텍스트");
				other_context.with(|_engine| ());
			});
		});
		poll_tasks(&mut engine, &mut scene_list, None);
	}
}
