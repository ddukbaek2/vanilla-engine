//==============================================================================
//! 씬.
//! - 원본은 Scene 클래스를 상속해 create / tick / draw 를 덮어쓴다.
//! - 러스트 판은 Scene 트레이트를 구현하고, 씬 공통 상태(노드 트리, 터치 대상)는 SceneBase 에 둔다.
//! - 노드는 SceneBase 의 NodeTree 가 들고, 게임 코드는 NodeId 핸들로 가리킨다. (get_node_tree_mut().create_child(...))
//!   원본의 super.tick() / super.draw() 자리는 get_scene_base_mut().tick() / .draw() 를 부른다.
//! - 원본 GameScene 의 pre_draw (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 거는 것)는 SceneBase::draw_scene_background 로 옮겼다.
//! - 트윈은 원본처럼 씬이 목록으로 들고(start_tween), tick 에서 노드 → 터치 다음에 돌리며 끝난 것을 뺀다.
//!   원본은 트윈 객체로 가리키지만, 러스트 판은 트윈을 넘겨받고 TweenId 를 돌려준다. (stop_tween(tween_id))
//! - 원본 SceneManager.unloadScene(scene) 은 씬 객체로 가리킨다. 러스트 판은 엔진이 씬을 들고 있어 load_scene 이 돌려주는 SceneId 나
//!   씬 공통 상태(&SceneBase, 원본처럼 씬으로)로 가리킨다. 씬 스스로는 get_scene_base().get_scene_id() 로 자기 핸들을 안다.
//! - 내릴 때는 원본처럼 finalize 다음 unload 를 부르고, 러스트 판은 씬을 버리기 직전에 destroy 를 더 부른다.
//! - 원본 async load(engine) 자리: create 에서 Engine::spawn_scene_task 로 불러오기 작업을 맡기고 track_task 로 씬에 맡긴다.
//!   작업 안에서는 원본처럼 await 로 애셋을 기다리고, SceneContext::with 안에서 씬과 엔진을 만진다. 맡긴 작업과 애셋이 모두 끝나야 initialize 를 부른다.
//!   애셋 손잡이를 track_asset 으로 맡기는 동기 꼴도 그대로 된다. (둘을 섞어도 된다)
//!   원본 await unload(engine) 자리: unload 에서 맡긴 작업(track_task)이 끝날 때까지 씬은 목록에 남아 tick, draw 를 받고, 끝나면 목록에서 빠진다.
//!   그 뒤에도 그 씬의 작업이 남았으면 원본의 Promise 사슬처럼 다 끝날 때까지 씬 값이 살고(tick, draw 는 받지 않는다), 불러오는 도중에 내린 씬은
//!   원본 SceneManager.loadScene 처럼 불러오기가 끝나면 initialize 까지 받는다. 다 끝나면 엔진이 destroy 를 부르고 버린다.
//! - 원본 Scene.load 가 루트를 뷰 크기로 맞추는 것(자리 0, 왼쪽 위 피벗, 콘텐츠 크기는 뷰 크기)은 엔진이 create 바로 뒤에 한다.
//!   원본 SceneManager.loadScene 처럼 맡긴 애셋이 다 끝난 뒤에도 로딩 화면(draw_on_load)을 최소 0.3초 더 보이고 initialize 를 부른다.
//! - 씬의 터치 훅(touch_press, touch_move, touch_release, touch_cancel, touch_wheel)은 원본 Scene.tickTouch 차례로 엔진이 tick 뒤에 부른다.
//!   SceneBase.tick 은 씬 꼴과 상관없이 가장 앞의 누를 수 있는 노드에도 넘긴다. (원본 Scene.tickTouch 가 씬의 TouchRaycaster 로 넘기는 것과 같다)
//! - 페이지 복원 훅과 터치 훅은 원본과 달리 엔진을 받는다. (전역 엔진이 없다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetLoadHandle};
use crate::core::task::{PromiseResolver, TaskHandle};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree, TouchKind};
use crate::core::node::worldnode::WorldNode;
use crate::core::tween::{Tween, TweenId};

//==============================================================================
/// 씬이 기다리는 읽기 하나. (애셋 읽기 손잡이, 또는 오디오 애셋)
//==============================================================================
enum TrackedLoad {
	AssetLoadHandle(AssetLoadHandle),
	AudioAsset(Rc<AudioAsset>),
	Task(TaskHandle),
}

impl TrackedLoad {
	//==============================================================================
	/// 끝났는지. (읽혔거나 실패했거나)
	//==============================================================================
	fn is_finished(&self) -> bool {
		match self {
			TrackedLoad::AssetLoadHandle(load_handle) => {
				load_handle.is_finished()
			}
			TrackedLoad::AudioAsset(audio_asset) => {
				audio_asset.is_loaded() || audio_asset.get_load_error().is_some()
			}
			TrackedLoad::Task(task_handle) => {
				task_handle.is_finished()
			}
		}
	}
}

//==============================================================================
/// 씬 핸들. (Engine::load_scene 이 돌려주고 Engine::unload_scene 이 받는다)
///
/// # 예제
/// 씬 둘을 불러 핸들로 하나만 내리기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, Scene, SceneBase, SceneId};
///
/// struct EmptyScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for EmptyScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
/// }
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// let first_scene_id: SceneId = engine.load_scene(Box::new(EmptyScene { scene_base: SceneBase::new() }));
/// let second_scene_id: SceneId = engine.load_scene(Box::new(EmptyScene { scene_base: SceneBase::new() }));
/// assert_ne!(first_scene_id, second_scene_id);
/// engine.unload_scene(second_scene_id);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SceneId(pub(crate) u64);

//==============================================================================
/// 씬 공통 상태에서 그 씬의 핸들. (원본처럼 씬으로 unload_scene 을 부를 때, engine.unload_scene(&self.scene_base))
/// - 아직 불러오지 않은 씬은 어느 씬도 가리키지 않는 핸들(0, 엔진은 1 부터 매긴다)이 되어 내릴 것이 없다.
//==============================================================================
impl From<&SceneBase> for SceneId {
	//==============================================================================
	/// 씬 공통 상태의 핸들 꺼내기. (불러오기 전이면 0 번 핸들)
	//==============================================================================
	fn from(scene_base: &SceneBase) -> SceneId {
		scene_base.get_scene_id().unwrap_or(SceneId(0))
	}
}

//==============================================================================
/// 씬 공통 상태.
/// - 노드 트리, 트윈 목록, 맡긴 애셋과 작업, 터치 대상, 배경색을 든다. 씬 구조체가 하나 들고 get_scene_base 로 내준다.
///
/// # 예제
/// 노드를 만들고 트윈으로 움직이기
/// ```rust
/// use vanilla::{SceneBase, Tween, TweenProperty, Vector2};
///
/// let mut scene_base = SceneBase::new();
/// let root_id = scene_base.get_root_id();
/// let player_id = scene_base.get_node_tree_mut().create_child(root_id, "player");
/// let tween = Tween::create_node_tween(player_id, TweenProperty::LocalPosition, Vector2::zero(), Vector2::create(100.0, 0.0), 1.0);
/// let tween_id = scene_base.start_tween(tween);
/// assert_eq!(scene_base.get_tween_count(), 1);
/// scene_base.tick_tweens(0.5);
/// assert_eq!(scene_base.get_node_tree()[player_id].get_local_position(), Vector2::create(50.0, 0.0));
/// scene_base.tick_tweens(0.5);
/// assert_eq!(scene_base.get_tween_count(), 0);
/// assert!(scene_base.get_tween(tween_id).is_none());
/// ```
//==============================================================================
pub struct SceneBase {
	node_tree: NodeTree,
	tween_list: Vec<Tween>,
	is_loaded: bool,
	is_gizmo_visible: bool,
	scene_background_color: Color,
	pressed_node_id: Option<NodeId>,
	tracked_load_list: Vec<TrackedLoad>,
	/// 내리기를 시작했는지. (그 뒤로 맡긴 것은 내리기 작업이다, 원본 await scene.unload 자리)
	is_unload_started: bool,
	tracked_unload_list: Vec<TrackedLoad>,
	has_ever_touched: bool,
	first_touch_callback: Option<Box<dyn FnMut()>>,
	/// 노드에 터치를 넘기는지. (원본 Scene 의 터치 레이캐스터가 있는지, 기본 참)
	is_touch_dispatch_enabled: bool,
	scene_id: Option<SceneId>,
	/// 이번 프레임의 휠을 받아 쓴 컴포넌트가 있었는지.
	is_wheel_handled: bool,
	/// 맡긴 애셋이 다 끝난 것을 엔진이 처음 본 시각. (엔진 시작 뒤 초, 원본 loadScene 의 0.3초 더 기다림을 잰다)
	tracked_assets_finished_time: Option<f64>,
	/// initialize 뒤에 풀 Promise. (Engine::load_scene_async, 원본 loadScene 이 돌려주는 Promise)
	load_resolver_list: Vec<PromiseResolver<SceneId>>,
}

impl SceneBase {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 새 씬 공통 상태. (루트 하나인 노드 트리, 배경색 검정, 불러오기 전, 기즈모 끔)
	//==============================================================================
	pub fn new() -> SceneBase {
		SceneBase {
			node_tree: NodeTree::new(),
			tween_list: Vec::new(),
			is_loaded: false,
			is_gizmo_visible: false,
			scene_background_color: Color::black(),
			pressed_node_id: None,
			tracked_load_list: Vec::new(),
			is_unload_started: false,
			tracked_unload_list: Vec::new(),
			has_ever_touched: false,
			first_touch_callback: None,
			is_touch_dispatch_enabled: true,
			scene_id: None,
			is_wheel_handled: false,
			tracked_assets_finished_time: None,
			load_resolver_list: Vec::new(),
		}
	}

	//==============================================================================
	/// 이번 프레임의 휠을 받아 쓴 컴포넌트(스크롤 칸 등)가 있었는지. (tick_touch 뒤에 읽는다, 없으면 게임이 지도 확대 등에 쓴다)
	///
	/// # 돌려줌
	/// 이번 프레임의 휠을 컴포넌트가 썼으면 true.
	//==============================================================================
	pub fn is_wheel_handled(&self) -> bool {
		self.is_wheel_handled
	}

	//==============================================================================
	/// 씬 핸들. (엔진에 load_scene 으로 넘긴 뒤부터 있다)
	///
	/// # 돌려줌
	/// 이 씬의 핸들. (load_scene 전이면 None)
	//==============================================================================
	pub fn get_scene_id(&self) -> Option<SceneId> {
		self.scene_id
	}

	//==============================================================================
	/// 씬 핸들 설정. (Engine::load_scene 이 부른다)
	//==============================================================================
	pub(crate) fn set_scene_id(&mut self, scene_id: SceneId) {
		self.scene_id = Some(scene_id);
	}

	//==============================================================================
	/// 첫 터치 콜백 설정. (처음 누를 때 한 번. 웹에서 오디오 잠금을 푸는 자리, 원본 TouchRaycaster)
	///
	/// # 인자
	/// - `first_touch_callback`: 처음 누를 때 부를 함수. (앞서 건 함수는 바꾼다)
	//==============================================================================
	pub fn set_first_touch_callback(&mut self, first_touch_callback: Box<dyn FnMut()>) {
		self.first_touch_callback = Some(first_touch_callback);
	}

	//==============================================================================
	/// 노드에 터치 넘기기 켜고 끄기. (원본 Scene.setTouchDispatchEnabled, setTouchRaycaster(null) 과 같은 효과)
	/// - 끄면 tick_touch 가 노드에 누름, 이동, 뗌, 휠을 넘기지 않고(첫 터치 콜백도 부르지 않는다) 씬의 터치 훅만 불린다. (훅에서 가로채 직접 넘길 때)
	///
	/// # 인자
	/// - `is_enabled`: 노드에 넘기면 참. (기본 참)
	///
	/// # 예제
	/// 씬 기본 넘기기를 끄고 훅에서 직접 받기
	/// ```rust
	/// use vanilla::SceneBase;
	///
	/// let mut scene_base = SceneBase::new();
	/// scene_base.set_touch_dispatch_enabled(false);
	/// assert!(!scene_base.is_touch_dispatch_enabled());
	/// ```
	//==============================================================================
	pub fn set_touch_dispatch_enabled(&mut self, is_enabled: bool) {
		self.is_touch_dispatch_enabled = is_enabled;
	}

	//==============================================================================
	/// 노드에 터치를 넘기는지.
	///
	/// # 돌려줌
	/// 넘기면 참.
	//==============================================================================
	pub fn is_touch_dispatch_enabled(&self) -> bool {
		self.is_touch_dispatch_enabled
	}

	//==============================================================================
	/// 지금 눌려 있는 노드. (원본 TouchRaycaster.getTouchTarget)
	///
	/// # 돌려줌
	/// 누를 때 찾은 맨 위의 누를 수 있는 노드. (누르고 있지 않거나 빈 곳을 눌렀으면 None)
	//==============================================================================
	pub fn get_touch_target(&self) -> Option<NodeId> {
		self.pressed_node_id
	}

	//==============================================================================
	/// 애셋 맡기기. (create 에서 load 를 요청한 애셋. 맡긴 것이 다 읽히면 엔진이 initialize 를 부른다)
	///
	/// # 인자
	/// - `asset`: load 를 요청한 애셋. (읽기 손잡이가 없는 애셋은 맡기지 않는다)
	//==============================================================================
	pub fn track_asset(&mut self, asset: &dyn Asset) {
		if let Some(load_handle) = asset.get_asset_base().get_load_handle() {
			self.push_tracked_load(TrackedLoad::AssetLoadHandle(load_handle.clone()));
		}
	}

	//==============================================================================
	/// 읽기 손잡이 맡기기. (애셋 꼴이 아닌 것)
	///
	/// # 인자
	/// - `load_handle`: 기다릴 읽기 손잡이. (복제해 든다)
	//==============================================================================
	pub fn track_load_handle(&mut self, load_handle: &AssetLoadHandle) {
		self.push_tracked_load(TrackedLoad::AssetLoadHandle(load_handle.clone()));
	}

	//==============================================================================
	/// 오디오 애셋 맡기기. (engine.load_audio_asset 으로 받은 것. 웹은 디코딩이 끝나야 읽힌 것으로 친다)
	///
	/// # 인자
	/// - `audio_asset`: 기다릴 오디오 애셋. (읽혔거나 실패하면 끝난 것으로 친다)
	//==============================================================================
	pub fn track_audio_asset(&mut self, audio_asset: &Rc<AudioAsset>) {
		self.push_tracked_load(TrackedLoad::AudioAsset(audio_asset.clone()));
	}

	//==============================================================================
	/// 작업 맡기기. (원본 async load 자리, create 에서 spawn_scene_task 로 맡긴 불러오기 작업. 끝나야 엔진이 initialize 를 부른다)
	/// - unload 에서 맡기면 원본 await scene.unload 처럼 끝나야 내리기가 끝난다. (그동안 씬은 목록에 남는다)
	///
	/// # 인자
	/// - `task_handle`: 기다릴 작업의 손잡이. (spawn_scene_task, spawn_task 가 돌려준 것)
	///
	/// # 예제
	/// 원본 async load 자리: create 에서 불러오기 작업을 맡기기
	/// ```rust,no_run
	/// use vanilla::{Engine, Scene, SceneBase, SceneContext};
	///
	/// struct StageScene {
	/// 	scene_base: SceneBase,
	/// 	stage_byte_count: usize,
	/// }
	///
	/// impl Scene for StageScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn create(&mut self, engine: &mut Engine) {
	/// 		let task_handle = engine.spawn_scene_task(self, |mut scene_context: SceneContext<StageScene>| async move {
	/// 			let load_handle = scene_context.with(|_scene, engine| engine.request_asset_bytes("examples/assets/scenedocument.scene.json"));
	/// 			if let Ok(bytes) = load_handle.await {
	/// 				scene_context.with(|scene, _engine| {
	/// 					scene.stage_byte_count = bytes.len();
	/// 				});
	/// 			}
	/// 		});
	/// 		self.scene_base.track_task(&task_handle);
	/// 	}
	///
	/// 	fn initialize(&mut self, _engine: &mut Engine) {
	/// 		println!("{}", self.stage_byte_count);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn track_task(&mut self, task_handle: &TaskHandle) {
		self.push_tracked_load(TrackedLoad::Task(task_handle.clone()));
	}

	//==============================================================================
	/// 맡긴 것 넣기. (내리기를 시작했으면 내리기 목록에)
	//==============================================================================
	fn push_tracked_load(&mut self, tracked_load: TrackedLoad) {
		if self.is_unload_started {
			self.tracked_unload_list.push(tracked_load);
		}
		else {
			self.tracked_load_list.push(tracked_load);
		}
	}

	//==============================================================================
	/// 내리기 시작. (엔진이 unload 를 부르기 바로 앞에 부른다, 그 뒤로 맡긴 것은 내리기가 기다린다)
	//==============================================================================
	pub(crate) fn begin_unload_tracking(&mut self) {
		self.is_unload_started = true;
	}

	//==============================================================================
	/// unload 에서 맡긴 것이 다 끝났는지. (읽혔거나 실패했거나 작업이 끝났거나)
	///
	/// # 돌려줌
	/// unload 에서 맡긴 것이 모두 끝났으면 true. (맡긴 것이 없으면 true)
	//==============================================================================
	pub fn are_tracked_unloads_finished(&self) -> bool {
		self.tracked_unload_list.iter().all(|tracked_load| tracked_load.is_finished())
	}

	//==============================================================================
	/// initialize 뒤에 풀 Promise 맡기기. (Engine::load_scene_async 가 부른다)
	//==============================================================================
	pub(crate) fn add_load_resolver(&mut self, load_resolver: PromiseResolver<SceneId>) {
		self.load_resolver_list.push(load_resolver);
	}

	//==============================================================================
	/// initialize 뒤에 맡긴 Promise 풀기. (엔진이 initialize 바로 뒤에 부른다)
	//==============================================================================
	pub(crate) fn resolve_load_resolvers(&mut self) {
		let scene_id = SceneId::from(&*self);
		for load_resolver in std::mem::take(&mut self.load_resolver_list) {
			load_resolver.resolve(scene_id);
		}
	}

	//==============================================================================
	/// 맡긴 애셋이 다 끝났는지. (읽혔거나 실패했거나. 실패한 애셋 때문에 씬이 멈추지 않게 한다)
	///
	/// # 돌려줌
	/// 불러오기에 맡긴 애셋과 작업이 모두 끝났으면 true. (맡긴 것이 없으면 true)
	//==============================================================================
	pub fn are_tracked_assets_finished(&self) -> bool {
		self.tracked_load_list.iter().all(|tracked_load| tracked_load.is_finished())
	}

	//==============================================================================
	/// 맡긴 애셋이 하나를 빼고 다 끝났는지. (GameSceneBase 가 로딩 문을 빼고 원본 loadAssets 가 끝났는지 볼 때)
	///
	/// # 인자
	/// - `excluded_load_handle`: 끝났는지 보지 않을 읽기 손잡이. (같은 읽기를 가리키는 손잡이를 모두 뺀다)
	///
	/// # 돌려줌
	/// 그 손잡이를 뺀 나머지가 모두 끝났으면 true.
	//==============================================================================
	pub fn are_tracked_assets_finished_except(&self, excluded_load_handle: &AssetLoadHandle) -> bool {
		self.tracked_load_list.iter().all(|tracked_load| {
			match tracked_load {
				TrackedLoad::AssetLoadHandle(load_handle) => {
					load_handle.is_same_load(excluded_load_handle) || load_handle.is_finished()
				}
				TrackedLoad::AudioAsset(_) | TrackedLoad::Task(_) => {
					tracked_load.is_finished()
				}
			}
		})
	}

	//==============================================================================
	/// 맡긴 애셋 가운데 끝난 비율. (로딩 화면 진행 막대용, 맡긴 것이 없으면 1)
	///
	/// # 돌려줌
	/// 끝난 수 / 맡긴 수. (0 ~ 1, 실패한 것도 끝난 것으로 센다)
	///
	/// # 예제
	/// 읽기 손잡이 둘의 진행 비율
	/// ```rust
	/// use vanilla::{AssetLoadHandle, SceneBase};
	///
	/// let mut scene_base = SceneBase::new();
	/// let scene_load_handle = AssetLoadHandle::create_pending("examples/assets/scenedocument.scene.json");
	/// let atlas_load_handle = AssetLoadHandle::create_pending("examples/assets/atlas/icons.json");
	/// scene_base.track_load_handle(&scene_load_handle);
	/// scene_base.track_load_handle(&atlas_load_handle);
	/// assert_eq!(scene_base.get_tracked_asset_progress(), 0.0);
	/// scene_load_handle.finish(Ok(b"{}".to_vec()));
	/// assert_eq!(scene_base.get_tracked_asset_progress(), 0.5);
	/// atlas_load_handle.finish(Err(String::from("not found")));
	/// assert!(scene_base.are_tracked_assets_finished());
	/// ```
	//==============================================================================
	pub fn get_tracked_asset_progress(&self) -> f64 {
		if self.tracked_load_list.is_empty() {
			return 1.0;
		}
		let finished_count = self.tracked_load_list.iter().filter(|tracked_load| tracked_load.is_finished()).count();
		finished_count as f64 / self.tracked_load_list.len() as f64
	}

	//==============================================================================
	/// 맡긴 애셋 비우기. (initialize 직전에 엔진이 부른다)
	//==============================================================================
	pub fn clear_tracked_assets(&mut self) {
		self.tracked_load_list.clear();
		self.tracked_assets_finished_time = None;
	}

	//==============================================================================
	/// 맡긴 애셋이 다 끝난 시각 적기. (처음 부를 때의 시각을 적고, 그 뒤로는 적힌 시각을 돌려준다. 엔진이 부른다)
	//==============================================================================
	pub(crate) fn mark_tracked_assets_finished(&mut self, current_time: f64) -> f64 {
		*self.tracked_assets_finished_time.get_or_insert(current_time)
	}

	//==============================================================================
	/// 한 번이라도 눌린 적이 있는지. (원본 Scene.hasEverTouched, 처음 누를 때 참이 된다)
	///
	/// # 돌려줌
	/// 이 씬에서 한 번이라도 눌렀으면 true.
	//==============================================================================
	pub fn has_ever_touched(&self) -> bool {
		self.has_ever_touched
	}

	//==============================================================================
	/// 노드 트리.
	///
	/// # 돌려줌
	/// 이 씬의 노드 트리.
	//==============================================================================
	pub fn get_node_tree(&self) -> &NodeTree {
		&self.node_tree
	}

	//==============================================================================
	/// 노드 트리. (변경용)
	///
	/// # 돌려줌
	/// 이 씬의 노드 트리. (노드 만들기, 붙이기, 지우기)
	//==============================================================================
	pub fn get_node_tree_mut(&mut self) -> &mut NodeTree {
		&mut self.node_tree
	}

	//==============================================================================
	/// 루트 노드 핸들.
	///
	/// # 돌려줌
	/// 노드 트리의 루트 노드 핸들.
	//==============================================================================
	pub fn get_root_id(&self) -> NodeId {
		self.node_tree.get_root_id()
	}

	//==============================================================================
	/// 루트 노드.
	///
	/// # 돌려줌
	/// 루트 노드. (엔진이 create 뒤에 뷰 크기로 맞춘다)
	//==============================================================================
	pub fn get_root(&self) -> &WorldNode {
		let root_id = self.node_tree.get_root_id();
		&self.node_tree[root_id]
	}

	//==============================================================================
	/// 루트 노드. (변경용)
	///
	/// # 돌려줌
	/// 루트 노드.
	//==============================================================================
	pub fn get_root_mut(&mut self) -> &mut WorldNode {
		let root_id = self.node_tree.get_root_id();
		&mut self.node_tree[root_id]
	}

	//==============================================================================
	/// 로드 완료 설정.
	///
	/// # 인자
	/// - `is_loaded`: 불러오기를 마쳤는지. (엔진이 initialize 뒤에 true 로 둔다)
	//==============================================================================
	pub fn set_loaded(&mut self, is_loaded: bool) {
		self.is_loaded = is_loaded;
	}

	//==============================================================================
	/// 로드 완료 여부.
	///
	/// # 돌려줌
	/// initialize 를 마쳤으면 true. (그 전에는 엔진이 tick, draw 대신 draw_on_load 를 부른다)
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		self.is_loaded
	}

	//==============================================================================
	/// 기즈모 표시 설정.
	///
	/// # 인자
	/// - `is_gizmo_visible`: true 면 엔진이 post_draw 뒤에 draw_gizmos 를 부른다.
	//==============================================================================
	pub fn set_gizmo_visible(&mut self, is_gizmo_visible: bool) {
		self.is_gizmo_visible = is_gizmo_visible;
	}

	//==============================================================================
	/// 기즈모 표시 여부.
	///
	/// # 돌려줌
	/// 기즈모를 그리면 true. (기본 false)
	//==============================================================================
	pub fn is_gizmo_visible(&self) -> bool {
		self.is_gizmo_visible
	}

	//==============================================================================
	/// 배경색 설정. (draw_scene_background 가 칠한다)
	///
	/// # 인자
	/// - `color`: 배경색. (Color 나 "#202020" 같은 색 글)
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<Color>) {
		self.scene_background_color = color.into();
	}

	//==============================================================================
	/// 배경색 반환.
	///
	/// # 돌려줌
	/// 배경색. (기본 검정)
	//==============================================================================
	pub fn get_scene_background_color(&self) -> Color {
		self.scene_background_color
	}

	//==============================================================================
	/// 기본 갱신. (루트 노드 → 터치 → 트윈)
	///
	/// # 인자
	/// - `engine`: 엔진. (입력과 매니저를 읽는다)
	/// - `time_delta`: 배율을 곱한 프레임 간격. (초)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, time_delta: f64) {
		self.node_tree.tick(engine, time_delta);
		self.tick_touch(engine);
		self.tick_tweens(time_delta);
	}

	//==============================================================================
	/// 트윈 목록 갱신. (원본처럼 뒤에서부터 돌고, 끝난 트윈은 목록에서 뺀다)
	/// - 같은 속성을 둘이 움직이면 먼저 시작한 트윈이 나중에 돌아 이긴다. (원본과 같음)
	///
	/// # 인자
	/// - `time_delta`: 진행할 시간. (초)
	//==============================================================================
	pub fn tick_tweens(&mut self, time_delta: f64) {
		let mut tween_index = self.tween_list.len();
		while tween_index > 0 {
			tween_index -= 1;
			let tween = &mut self.tween_list[tween_index];
			tween.tick(&mut self.node_tree, time_delta);
			if tween.is_finished() {
				self.tween_list.remove(tween_index);
			}
		}
	}

	//==============================================================================
	/// 트윈 시작. (트윈을 넘겨받아 start 하고 목록 끝에 붙인다. 가리킬 핸들을 돌려준다)
	///
	/// # 인자
	/// - `tween`: 돌릴 트윈. (씬이 소유한다)
	///
	/// # 돌려줌
	/// 그 트윈의 핸들. (stop_tween, get_tween 에 넘긴다)
	//==============================================================================
	pub fn start_tween(&mut self, mut tween: Tween) -> TweenId {
		let tween_id = tween.get_tween_id();
		tween.start();
		self.tween_list.push(tween);
		tween_id
	}

	//==============================================================================
	/// 트윈 중단. (완료 콜백 없이 멈추고 목록에서 뺀다. 이미 끝나 빠졌으면 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `tween_id`: 멈출 트윈의 핸들. (start_tween 이 돌려준 것)
	//==============================================================================
	pub fn stop_tween(&mut self, tween_id: TweenId) {
		let tween_index = match self.tween_list.iter().position(|tween| tween.get_tween_id() == tween_id) {
			Some(tween_index) => {
				tween_index
			}
			None => {
				return;
			}
		};
		self.tween_list[tween_index].stop();
		self.tween_list.remove(tween_index);
	}

	//==============================================================================
	/// 모든 트윈 중단.
	//==============================================================================
	pub fn stop_all_tweens(&mut self) {
		for tween in self.tween_list.iter_mut() {
			tween.stop();
		}
		self.tween_list.clear();
	}

	//==============================================================================
	/// 트윈 반환. (끝나서 목록에서 빠졌으면 None. 원본에서 트윈 객체를 들고 묻던 자리)
	///
	/// # 인자
	/// - `tween_id`: 찾을 트윈의 핸들.
	///
	/// # 돌려줌
	/// 돌고 있는 그 트윈. (끝났거나 멈춰 빠졌으면 None)
	//==============================================================================
	pub fn get_tween(&self, tween_id: TweenId) -> Option<&Tween> {
		self.tween_list.iter().find(|tween| tween.get_tween_id() == tween_id)
	}

	//==============================================================================
	/// 트윈 반환. (변경용)
	///
	/// # 인자
	/// - `tween_id`: 찾을 트윈의 핸들.
	///
	/// # 돌려줌
	/// 돌고 있는 그 트윈. (끝났거나 멈춰 빠졌으면 None)
	//==============================================================================
	pub fn get_tween_mut(&mut self, tween_id: TweenId) -> Option<&mut Tween> {
		self.tween_list.iter_mut().find(|tween| tween.get_tween_id() == tween_id)
	}

	//==============================================================================
	/// 돌고 있는 트윈 수.
	///
	/// # 돌려줌
	/// 목록의 트윈 수. (대기 중인 것 포함)
	//==============================================================================
	pub fn get_tween_count(&self) -> usize {
		self.tween_list.len()
	}

	//==============================================================================
	/// 터치 갱신. (눌린 곳 맨 위의 interactable 노드를 찾아 눌림 / 이동 / 뗌을 전달)
	/// - 휠은 누름과 상관없이 그 자리의 가장 앞 스크롤 칸이나 누를 수 있는 노드부터 부모 쪽으로 넘긴다.
	///
	/// # 인자
	/// - `engine`: 엔진. (입력 매니저의 뷰 입력 위치와 누름, 뗌, 휠을 읽는다)
	//==============================================================================
	pub fn tick_touch(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let view_input_position = input_manager.get_view_input_position();
		self.is_wheel_handled = false;
		// 넘기기를 껐으면 노드에 넘기지 않는다. (원본 setTouchRaycaster(null))
		if !self.is_touch_dispatch_enabled {
			return;
		}

		// 휠. (누름과 상관없이 그 자리의 가장 앞 스크롤 칸이나 누를 수 있는 노드부터 부모 쪽으로)
		let wheel_delta = Vector2::create(input_manager.get_wheel_delta_x(), input_manager.get_wheel_delta_y());
		self.is_wheel_handled = false;
		if wheel_delta.x != 0.0 || wheel_delta.y != 0.0 {
			self.is_wheel_handled = self.node_tree.dispatch_wheel_at(view_input_position, engine, wheel_delta);
		}

		if input_manager.is_touch_pressed() {
			if !self.has_ever_touched {
				self.has_ever_touched = true;
				if let Some(first_touch_callback) = self.first_touch_callback.as_mut() {
					first_touch_callback();
				}
			}
			let found_id = self.node_tree.find_topmost_interactable_node(view_input_position);
			if let Some(found_id) = found_id {
				self.node_tree.dispatch_touch(found_id, engine, TouchKind::Press, view_input_position);
			}
			self.pressed_node_id = found_id;
		}

		let pressed_node_id = match self.pressed_node_id {
			Some(pressed_node_id) => {
				pressed_node_id
			}
			None => {
				return;
			}
		};
		// 원본처럼 뗌이 먼저, 누르고 있음은 누른 프레임에는 넘기지 않는다. (한 프레임 안에 눌렀다 떼면 누름 뒤에 바로 뗌)
		if input_manager.is_touch_released() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Release, view_input_position);
			self.pressed_node_id = None;
		}
		else if input_manager.is_touch_moved() && !input_manager.is_touch_pressed() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Moved, view_input_position);
		}
		else if input_manager.is_touch_cancelled() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Cancel, view_input_position);
			self.pressed_node_id = None;
		}
	}

	//==============================================================================
	/// 기본 출력. (루트 노드 재귀)
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn draw(&mut self, graphic: &mut Graphic) {
		self.node_tree.draw(graphic);
	}

	//==============================================================================
	/// 캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. (원본 GameScene.preDraw)
	///
	/// # 인자
	/// - `engine`: 엔진. (뷰 매니저의 캔버스 크기와 뷰 영역을 읽는다)
	/// - `graphic`: 칠할 그래픽. (끝나면 뷰 좌표계가 걸려 있다)
	///
	/// # 예제
	/// pre_draw 에서 배경 칠하기
	/// ```rust,no_run
	/// use vanilla::{Engine, Graphic, Scene, SceneBase};
	///
	/// struct MainScene {
	/// 	scene_base: SceneBase,
	/// }
	///
	/// impl Scene for MainScene {
	/// 	fn get_scene_base(&self) -> &SceneBase {
	/// 		&self.scene_base
	/// 	}
	///
	/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
	/// 		&mut self.scene_base
	/// 	}
	///
	/// 	fn create(&mut self, _engine: &mut Engine) {
	/// 		self.scene_base.set_scene_background_color("#203040");
	/// 	}
	///
	/// 	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
	/// 		self.scene_base.draw_scene_background(engine, graphic);
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn draw_scene_background(&self, engine: &Engine, graphic: &mut Graphic) {
		let view_manager = engine.get_view_manager();
		let canvas_native_size = view_manager.get_canvas_native_size();

		view_manager.apply_canvas_native_rect(graphic);
		graphic.set_fill_color(self.scene_background_color);
		graphic.draw_rect(Rect::create(0.0, 0.0, canvas_native_size.x, canvas_native_size.y));

		view_manager.apply_view_rect(graphic);
	}
}

impl Default for SceneBase {
	//==============================================================================
	/// 기본값. (SceneBase::new 와 같다)
	//==============================================================================
	fn default() -> SceneBase {
		SceneBase::new()
	}
}

//==============================================================================
/// 씬을 그 타입으로 꺼내기. (SceneContext 가 &mut dyn Scene 을 작업을 맡긴 씬의 타입으로 되돌릴 때 쓴다, 모든 씬에 저절로 붙는다)
///
/// # 예제
/// `Box<dyn Scene>` 을 씬 타입으로 되돌리기
/// ```rust
/// use vanilla::core::scene::SceneAny;
/// use vanilla::{Scene, SceneBase};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// 	score: u32,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
/// }
///
/// let mut boxed_scene: Box<dyn Scene> = Box::new(MainScene { scene_base: SceneBase::new(), score: 7 });
/// let main_scene = boxed_scene.as_scene_any_mut().downcast_mut::<MainScene>().unwrap();
/// assert_eq!(main_scene.score, 7);
/// ```
//==============================================================================
pub trait SceneAny {
	//==============================================================================
	/// Any 로 빌리기.
	///
	/// # 돌려줌
	/// 이 씬을 `dyn Any` 로 빌린 것. (downcast_mut 로 씬 타입으로 되돌린다)
	//==============================================================================
	fn as_scene_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: Scene + 'static> SceneAny for T {
	//==============================================================================
	/// Any 로 빌리기.
	//==============================================================================
	fn as_scene_any_mut(&mut self) -> &mut dyn std::any::Any {
		self
	}
}

//==============================================================================
/// 씬 트레이트.
/// - get_scene_base, get_scene_base_mut 만 꼭 구현하고, 나머지 훅은 필요한 것만 덮어쓴다.
///
/// # 예제
/// 흰 사각형 하나를 그리는 첫 씬 (examples/quickstart.rs)
/// ```rust,no_run
/// use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
/// 		self.scene_base.draw_scene_background(engine, graphic);
/// 	}
///
/// 	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
/// 		graphic.set_fill_color(Colors::WHITE);
/// 		graphic.draw_rect(Rect::create(100.0, 100.0, 200.0, 120.0));
/// 	}
/// }
///
/// let mut engine_configuration = EngineConfiguration::new();
/// engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
/// let engine = Engine::new(engine_configuration);
/// let main_scene = MainScene {
/// 	scene_base: SceneBase::new(),
/// };
/// if let Err(error) = engine.run(Box::new(main_scene)) {
/// 	eprintln!("{}", error);
/// }
/// ```
///
/// 누른 자리 받기
/// ```rust,no_run
/// use vanilla::{Engine, Scene, SceneBase, Vector2};
///
/// struct TouchScene {
/// 	scene_base: SceneBase,
/// 	last_touch_position: Vector2,
/// }
///
/// impl Scene for TouchScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn touch_press(&mut self, _engine: &mut Engine, view_input_position: Vector2) {
/// 		self.last_touch_position = view_input_position;
/// 	}
/// }
/// ```
//==============================================================================
pub trait Scene: SceneAny {
	//==============================================================================
	/// 씬 공통 상태 반환.
	///
	/// # 돌려줌
	/// 씬이 든 씬 공통 상태.
	//==============================================================================
	fn get_scene_base(&self) -> &SceneBase;

	//==============================================================================
	/// 씬 공통 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 씬이 든 씬 공통 상태.
	//==============================================================================
	fn get_scene_base_mut(&mut self) -> &mut SceneBase;

	//==============================================================================
	/// 생성. (엔진이 씬을 불러올 때 한 번. 노드 트리와 애셋을 여기서 만든다)
	/// - 원본 async load 자리: engine.spawn_scene_task(self, |mut scene_context| async move { ... }) 로 맡긴 작업을 get_scene_base_mut().track_task 로 맡긴다.
	///
	/// # 인자
	/// - `_engine`: 엔진. (애셋 읽기, 작업 맡기기, 매니저)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 초기화. (create 에서 맡긴 애셋이 다 읽히고 0.3 초 뒤 한 번. 원본 initialize. 맡긴 것이 없어도 create 뒤 0.3 초를 기다린다)
	/// - 웹은 애셋이 나중에 오므로, 애셋을 쓰는 노드는 여기서 만든다. 데스크탑은 create 에서 만들어도 된다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 로딩 중 출력. (initialize 전까지 tick / draw 대신 불린다. 원본 drawOnLoad)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 로딩 화면을 그릴 그래픽.
	//==============================================================================
	fn draw_on_load(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 종료처리. (내릴 때 unload 앞에 한 번, 원본 finalize)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn finalize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 로딩 해제. (내릴 때 finalize 뒤에 한 번, 원본 async unload. 여기서 track_task 로 맡긴 작업이 끝나야 내리기가 끝난다)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn unload(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 해제. (엔진이 씬을 내릴 때 unload 뒤, 씬을 버리기 직전에 한 번. 그 씬의 작업이 남았으면 다 끝난 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 터치 누름. (원본 Scene.touchPress, 엔진이 tick 뒤에 부른다)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 누른 자리. (뷰 좌표)
	//==============================================================================
	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 이동. (누르고 있는 동안, 누른 프레임과 뗀 프레임에는 부르지 않는다)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 지금 자리. (뷰 좌표)
	//==============================================================================
	fn touch_move(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 뗌. (한 프레임 안에 눌렀다 떼면 누름 뒤에 바로)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 뗀 자리. (뷰 좌표)
	//==============================================================================
	fn touch_release(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 취소됨.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 취소된 때의 자리. (뷰 좌표)
	//==============================================================================
	fn touch_cancel(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 마우스 휠. (이번 프레임에 쌓인 휠이 있을 때)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 마우스 자리. (뷰 좌표)
	/// - `_wheel_delta`: 이번 프레임에 쌓인 휠 양. (x 가로, y 세로, 입력 매니저의 get_wheel_delta_x, get_wheel_delta_y)
	//==============================================================================
	fn touch_wheel(&mut self, _engine: &mut Engine, _view_input_position: Vector2, _wheel_delta: Vector2) {
	}

	//==============================================================================
	/// 페이지 프로세스 파기 뒤 복원. (Chromium 의 freeze 뒤 resume, bfcache 에서 돌아온 pageshow 등)
	/// - 엔진이 소리를 다시 깨우고 입력 상태를 비운 뒤 불러온 모든 씬에 부른다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn on_page_process_restored(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 페이지 가시성 복원. (백그라운드 → 포어그라운드, 웹 visibilitychange 의 visible, 데스크탑 창 가림이 풀림)
	/// - 엔진이 소리를 다시 깨우고 입력 상태를 비운 뒤 불러온 모든 씬에 부른다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 화면 영역 변경. (창 크기가 바뀌었을 때, 불러온 직후에도 한 번)
	///
	/// # 인자
	/// - `_engine`: 엔진. (뷰 매니저에 새 뷰 영역이 셈해져 있다)
	/// - `_canvas_native_size`: 창 안쪽 논리 크기. (뷰 매니저의 get_canvas_native_size)
	//==============================================================================
	fn resize(&mut self, _engine: &mut Engine, _canvas_native_size: Vector2) {
	}

	//==============================================================================
	/// 주기적 갱신.
	/// - 기본 구현은 씬 공통 상태의 tick 을 부른다. 덮어쓰면 get_scene_base_mut().tick(engine, time_delta) 를 직접 부른다.
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `time_delta`: 배율을 곱한 프레임 간격. (초, 시간 관리자의 get_time_delta)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.get_scene_base_mut().tick(engine, time_delta);
	}

	//==============================================================================
	/// 이전 출력.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn pre_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 출력.
	/// - 기본 구현은 씬 공통 상태의 draw 로 노드 트리를 그린다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.get_scene_base_mut().draw(graphic);
	}

	//==============================================================================
	/// 이후 출력.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 기즈모 출력.
	/// - 기즈모 표시(set_gizmo_visible)를 켠 씬에만 post_draw 뒤에 부른다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn draw_gizmos(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}
}
