//==============================================================================
// 씬.
// - 원본은 Scene 클래스를 상속해 create / tick / draw 를 덮어쓴다.
// - 러스트 판은 Scene 트레이트를 구현하고, 씬 공통 상태(노드 트리, 터치 대상)는 SceneBase 에 둔다.
// - 노드는 SceneBase 의 NodeTree 가 들고, 게임 코드는 NodeId 핸들로 가리킨다. (get_node_tree_mut().create_child(...))
//   원본의 super.tick() / super.draw() 자리는 get_scene_base_mut().tick() / .draw() 를 부른다.
// - 원본 GameScene 의 pre_draw (캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 거는 것)는 SceneBase::draw_scene_background 로 옮겼다.
// - 트윈은 원본처럼 씬이 목록으로 들고(start_tween), tick 에서 노드 → 터치 다음에 돌리며 끝난 것을 뺀다.
//   원본은 트윈 객체로 가리키지만, 러스트 판은 트윈을 넘겨받고 TweenId 를 돌려준다. (stop_tween(tween_id))
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::asset::{Asset, AssetLoadHandle};
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree, TouchKind};
use crate::core::node::worldnode::WorldNode;
use crate::core::tween::{Tween, TweenId};

//==============================================================================
// 씬이 기다리는 읽기 하나. (애셋 읽기 손잡이, 또는 오디오 애셋)
//==============================================================================
enum TrackedLoad {
	AssetLoadHandle(AssetLoadHandle),
	AudioAsset(Rc<AudioAsset>),
}

impl TrackedLoad {
	//==============================================================================
	// 끝났는지. (읽혔거나 실패했거나)
	//==============================================================================
	fn is_finished(&self) -> bool {
		match self {
			TrackedLoad::AssetLoadHandle(load_handle) => {
				load_handle.is_finished()
			}
			TrackedLoad::AudioAsset(audio_asset) => {
				audio_asset.is_loaded() || audio_asset.get_load_error().is_some()
			}
		}
	}
}

//==============================================================================
// 씬 공통 상태.
//==============================================================================
pub struct SceneBase {
	node_tree: NodeTree,
	tween_list: Vec<Tween>,
	is_loaded: bool,
	is_gizmo_visible: bool,
	scene_background_color: Color,
	pressed_node_id: Option<NodeId>,
	tracked_load_list: Vec<TrackedLoad>,
	has_ever_touched: bool,
	first_touch_callback: Option<Box<dyn FnMut()>>,
}

impl SceneBase {
	//==============================================================================
	// 생성.
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
			has_ever_touched: false,
			first_touch_callback: None,
		}
	}

	//==============================================================================
	// 첫 터치 콜백 설정. (처음 누를 때 한 번. 웹에서 오디오 잠금을 푸는 자리, 원본 TouchRaycaster)
	//==============================================================================
	pub fn set_first_touch_callback(&mut self, first_touch_callback: Box<dyn FnMut()>) {
		self.first_touch_callback = Some(first_touch_callback);
	}

	//==============================================================================
	// 지금 눌려 있는 노드. (원본 TouchRaycaster.get_touch_target)
	//==============================================================================
	pub fn get_touch_target(&self) -> Option<NodeId> {
		self.pressed_node_id
	}

	//==============================================================================
	// 애셋 맡기기. (create 에서 load 를 요청한 애셋. 맡긴 것이 다 읽히면 엔진이 initialize 를 부른다)
	//==============================================================================
	pub fn track_asset(&mut self, asset: &dyn Asset) {
		if let Some(load_handle) = asset.get_asset_base().get_load_handle() {
			self.tracked_load_list.push(TrackedLoad::AssetLoadHandle(load_handle.clone()));
		}
	}

	//==============================================================================
	// 읽기 손잡이 맡기기. (애셋 꼴이 아닌 것)
	//==============================================================================
	pub fn track_load_handle(&mut self, load_handle: &AssetLoadHandle) {
		self.tracked_load_list.push(TrackedLoad::AssetLoadHandle(load_handle.clone()));
	}

	//==============================================================================
	// 오디오 애셋 맡기기. (engine.load_audio_asset 으로 받은 것. 웹은 디코딩이 끝나야 읽힌 것으로 친다)
	//==============================================================================
	pub fn track_audio_asset(&mut self, audio_asset: &Rc<AudioAsset>) {
		self.tracked_load_list.push(TrackedLoad::AudioAsset(audio_asset.clone()));
	}

	//==============================================================================
	// 맡긴 애셋이 다 끝났는지. (읽혔거나 실패했거나. 실패한 애셋 때문에 씬이 멈추지 않게 한다)
	//==============================================================================
	pub fn are_tracked_assets_finished(&self) -> bool {
		self.tracked_load_list.iter().all(|tracked_load| tracked_load.is_finished())
	}

	//==============================================================================
	// 맡긴 애셋 가운데 끝난 비율. (로딩 화면 진행 막대용, 맡긴 것이 없으면 1)
	//==============================================================================
	pub fn get_tracked_asset_progress(&self) -> f32 {
		if self.tracked_load_list.is_empty() {
			return 1.0;
		}
		let finished_count = self.tracked_load_list.iter().filter(|tracked_load| tracked_load.is_finished()).count();
		finished_count as f32 / self.tracked_load_list.len() as f32
	}

	//==============================================================================
	// 맡긴 애셋 비우기. (initialize 직전에 엔진이 부른다)
	//==============================================================================
	pub fn clear_tracked_assets(&mut self) {
		self.tracked_load_list.clear();
	}

	//==============================================================================
	// 노드 트리.
	//==============================================================================
	pub fn get_node_tree(&self) -> &NodeTree {
		&self.node_tree
	}

	//==============================================================================
	// 노드 트리. (변경용)
	//==============================================================================
	pub fn get_node_tree_mut(&mut self) -> &mut NodeTree {
		&mut self.node_tree
	}

	//==============================================================================
	// 루트 노드 핸들.
	//==============================================================================
	pub fn get_root_id(&self) -> NodeId {
		self.node_tree.get_root_id()
	}

	//==============================================================================
	// 루트 노드.
	//==============================================================================
	pub fn get_root(&self) -> &WorldNode {
		let root_id = self.node_tree.get_root_id();
		&self.node_tree[root_id]
	}

	//==============================================================================
	// 루트 노드. (변경용)
	//==============================================================================
	pub fn get_root_mut(&mut self) -> &mut WorldNode {
		let root_id = self.node_tree.get_root_id();
		&mut self.node_tree[root_id]
	}

	//==============================================================================
	// 로드 완료 설정.
	//==============================================================================
	pub fn set_loaded(&mut self, is_loaded: bool) {
		self.is_loaded = is_loaded;
	}

	//==============================================================================
	// 로드 완료 여부.
	//==============================================================================
	pub fn is_loaded(&self) -> bool {
		self.is_loaded
	}

	//==============================================================================
	// 기즈모 표시 설정.
	//==============================================================================
	pub fn set_gizmo_visible(&mut self, is_gizmo_visible: bool) {
		self.is_gizmo_visible = is_gizmo_visible;
	}

	//==============================================================================
	// 기즈모 표시 여부.
	//==============================================================================
	pub fn is_gizmo_visible(&self) -> bool {
		self.is_gizmo_visible
	}

	//==============================================================================
	// 배경색 설정. (draw_scene_background 가 칠한다)
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<Color>) {
		self.scene_background_color = color.into();
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn get_scene_background_color(&self) -> Color {
		self.scene_background_color
	}

	//==============================================================================
	// 기본 갱신. (루트 노드 → 터치 → 트윈)
	//==============================================================================
	pub fn tick(&mut self, engine: &Engine, time_delta: f32) {
		self.node_tree.tick(engine, time_delta);
		self.tick_touch(engine);
		self.tick_tweens(time_delta);
	}

	//==============================================================================
	// 트윈 목록 갱신. (원본처럼 뒤에서부터 돌고, 끝난 트윈은 목록에서 뺀다)
	// - 같은 속성을 둘이 움직이면 먼저 시작한 트윈이 나중에 돌아 이긴다. (원본과 같음)
	//==============================================================================
	pub fn tick_tweens(&mut self, time_delta: f32) {
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
	// 트윈 시작. (트윈을 넘겨받아 start 하고 목록 끝에 붙인다. 가리킬 핸들을 돌려준다)
	//==============================================================================
	pub fn start_tween(&mut self, mut tween: Tween) -> TweenId {
		let tween_id = tween.get_tween_id();
		tween.start();
		self.tween_list.push(tween);
		tween_id
	}

	//==============================================================================
	// 트윈 중단. (완료 콜백 없이 멈추고 목록에서 뺀다. 이미 끝나 빠졌으면 아무것도 하지 않는다)
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
	// 모든 트윈 중단.
	//==============================================================================
	pub fn stop_all_tweens(&mut self) {
		for tween in self.tween_list.iter_mut() {
			tween.stop();
		}
		self.tween_list.clear();
	}

	//==============================================================================
	// 트윈 반환. (끝나서 목록에서 빠졌으면 None. 원본에서 트윈 객체를 들고 묻던 자리)
	//==============================================================================
	pub fn get_tween(&self, tween_id: TweenId) -> Option<&Tween> {
		self.tween_list.iter().find(|tween| tween.get_tween_id() == tween_id)
	}

	//==============================================================================
	// 트윈 반환. (변경용)
	//==============================================================================
	pub fn get_tween_mut(&mut self, tween_id: TweenId) -> Option<&mut Tween> {
		self.tween_list.iter_mut().find(|tween| tween.get_tween_id() == tween_id)
	}

	//==============================================================================
	// 돌고 있는 트윈 수.
	//==============================================================================
	pub fn get_tween_count(&self) -> usize {
		self.tween_list.len()
	}

	//==============================================================================
	// 터치 갱신. (눌린 곳 맨 위의 interactable 노드를 찾아 눌림 / 이동 / 뗌을 전달)
	//==============================================================================
	pub fn tick_touch(&mut self, engine: &Engine) {
		let input_manager = engine.get_input_manager();
		let view_input_position = input_manager.get_view_input_position();

		// 휠. (누름과 상관없이 그 자리 아래 노드부터 부모 쪽으로)
		let wheel_delta = Vector2::create(input_manager.get_wheel_delta_x(), input_manager.get_wheel_delta_y());
		if wheel_delta.x != 0.0 || wheel_delta.y != 0.0 {
			if let Some(hit_id) = self.node_tree.find_topmost_interactable_node(view_input_position) {
				self.node_tree.dispatch_wheel(hit_id, engine, wheel_delta);
			}
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
		if input_manager.is_touch_moved() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Moved, view_input_position);
		}
		if input_manager.is_touch_released() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Release, view_input_position);
			self.pressed_node_id = None;
		}
		else if input_manager.is_touch_cancelled() {
			self.node_tree.dispatch_touch(pressed_node_id, engine, TouchKind::Cancel, view_input_position);
			self.pressed_node_id = None;
		}
	}

	//==============================================================================
	// 기본 출력. (루트 노드 재귀)
	//==============================================================================
	pub fn draw(&mut self, graphic: &mut Graphic) {
		self.node_tree.draw(graphic);
	}

	//==============================================================================
	// 캔버스 전체를 배경색으로 칠하고 뷰 좌표계를 건다. (원본 GameScene.pre_draw)
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
	fn default() -> SceneBase {
		SceneBase::new()
	}
}

//==============================================================================
// 씬 트레이트.
//==============================================================================
pub trait Scene {
	//==============================================================================
	// 씬 공통 상태 반환.
	//==============================================================================
	fn get_scene_base(&self) -> &SceneBase;

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_scene_base_mut(&mut self) -> &mut SceneBase;

	//==============================================================================
	// 생성. (엔진이 씬을 불러올 때 한 번. 노드 트리와 애셋을 여기서 만든다)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 초기화. (create 에서 맡긴 애셋이 다 읽힌 뒤 한 번. 원본 initialize. 맡긴 것이 없으면 create 바로 뒤)
	// - 웹은 애셋이 나중에 오므로, 애셋을 쓰는 노드는 여기서 만든다. 데스크탑은 create 에서 만들어도 된다.
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 로딩 중 출력. (initialize 전까지 tick / draw 대신 불린다. 원본 draw_on_load)
	//==============================================================================
	fn draw_on_load(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 해제. (엔진이 씬을 내릴 때 한 번)
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 화면 영역 변경. (창 크기가 바뀌었을 때, 불러온 직후에도 한 번)
	//==============================================================================
	fn resize(&mut self, _engine: &mut Engine, _canvas_native_size: Vector2) {
	}

	//==============================================================================
	// 주기적 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		self.get_scene_base_mut().tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력.
	//==============================================================================
	fn pre_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.get_scene_base_mut().draw(graphic);
	}

	//==============================================================================
	// 이후 출력.
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 기즈모 출력.
	//==============================================================================
	fn draw_gizmos(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}
}
