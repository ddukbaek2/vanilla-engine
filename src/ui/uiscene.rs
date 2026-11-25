//==============================================================================
// UI 씬. (원본 ui/uiscene.js 의 UIScene)
// - GameScene 과 같은 공통 틀(개발자 도구, 터치 라우팅, 로딩 화면, 세이프 에어리어 인셋, 뷰 크기 변화 감지, 캔버스 배경 칠하기)을 가진 씬이다.
// - 제 LayoutSolver 를 들고 tick 마다 트리의 UINode 에 솔버를 붙이고(attach_pending_in_tree) 한 번 푼 뒤 노드에 옮긴다.
// - 화면 가이드 UINode(screen_node): 왼쪽 위 (0, 0) 고정, 너비와 높이는 편집 변수라 크기가 바뀔 때 suggest_value 로 고친다. (UIKit 의 UIWindow.bounds)
// - 세이프 에어리어 가이드 UINode(safe_area_layout_guide): 화면 가이드에서 세이프 에어리어 인셋만큼 안쪽. 레이아웃 때 인셋을 스스로 맞춘다.
//   (UIKit 의 UIView.safeAreaLayoutGuide)
// - 러스트 판은 상속 대신 씬이 UISceneBase 를 품고 UIScene 트레이트를 구현한 뒤 ui_scene! 한 줄로 Scene 을 얻는다. (GameScene, game_scene! 과 같은 꼴)
//     struct MenuScene { ui_scene_base: UISceneBase }
//     impl UIScene for MenuScene {
//         fn get_ui_scene_base(&self) -> &UISceneBase { &self.ui_scene_base }
//         fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase { &mut self.ui_scene_base }
//         fn initialize(&mut self, engine: &mut Engine) { /* UINode 를 붙인 노드를 만들고 get_safe_area_layout_guide 의 앵커로 제약을 건다 */ }
//     }
//     vanilla::ui_scene!(MenuScene);
// - 원본과 다른 것:
//   - 원본은 Scene 을 바로 상속해 GameScene 과 같은 코드를 따로 든다. 러스트 판은 공통 틀을 GameSceneBase 로 품고(세이프 에어리어 노드는 만들지 않는다)
//     솔버와 두 가이드만 더한다. 그래서 GameSceneBase 의 화면 페이드와 놀이 입력도 쓸 수 있다. (원본 UIScene 에는 없다)
//   - 원본은 create 에서 솔버와 가이드를 만든다. 러스트 판은 엔진이 필요 없어 생성할 때 만든다. 원본 load 끝의 "뷰 크기를 솔버에 바로 반영"은 initialize 에서 한다.
//   - 세이프 에어리어 인셋은 { top, left, bottom, right } 객체 대신 LayoutMargins 다. 솔버 오류는 Result<(), LayoutError> 로 돌려준다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::scene::SceneBase;
use crate::core::viewmanager::ViewScaleMode;
use crate::game::gamescene::GameSceneBase;
use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
use crate::ui::autolayout::layoutsolver::{LayoutError, LayoutSolver};
use crate::ui::autolayout::layoutstrength::LayoutStrength;
use crate::ui::uinode::{LayoutMargins, UINode};

//==============================================================================
// UI 씬의 바탕.
//==============================================================================
pub struct UISceneBase {
	game_scene_base: GameSceneBase,
	solver: LayoutSolver,
	screen_node: UINode,
	safe_area_layout_guide: UINode,
	safe_area_insets: LayoutMargins,
	safe_area_layout_guide_constraints: Vec<LayoutConstraint>,
}

impl UISceneBase {
	//==============================================================================
	// 생성. (솔버, 화면 가이드, 세이프 에어리어 가이드. 인셋 기본 0 이라 두 가이드가 같은 영역)
	//==============================================================================
	pub fn new() -> UISceneBase {
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_safe_area_node_enabled(false);
		let solver = LayoutSolver::new();
		let mut screen_node = UINode::new();
		let _ = screen_node.set_solver(Some(solver.clone()));
		let _ = screen_node.add_constraint(&screen_node.left_anchor().equal_to(0));
		let _ = screen_node.add_constraint(&screen_node.top_anchor().equal_to(0));
		let _ = solver.add_edit_variable(screen_node.get_width_variable(), LayoutStrength::STRONG);
		let _ = solver.add_edit_variable(screen_node.get_height_variable(), LayoutStrength::STRONG);
		let mut safe_area_layout_guide = UINode::new();
		let _ = safe_area_layout_guide.set_solver(Some(solver.clone()));
		let mut ui_scene_base = UISceneBase {
			game_scene_base,
			solver,
			screen_node,
			safe_area_layout_guide,
			safe_area_insets: LayoutMargins::create(0.0, 0.0, 0.0, 0.0),
			safe_area_layout_guide_constraints: Vec::new(),
		};
		let _ = ui_scene_base.rebuild_safe_area_layout_guide_constraints();
		ui_scene_base
	}

	//==============================================================================
	// 공통 틀 반환. (로딩 화면, 배경색, 페이드 같은 GameSceneBase 기능)
	//==============================================================================
	pub fn get_game_scene_base(&self) -> &GameSceneBase {
		&self.game_scene_base
	}

	//==============================================================================
	// 공통 틀 반환. (변경용)
	//==============================================================================
	pub fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
		&mut self.game_scene_base
	}

	//==============================================================================
	// 씬 공통 상태 반환.
	//==============================================================================
	pub fn get_scene_base(&self) -> &SceneBase {
		self.game_scene_base.get_scene_base()
	}

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	pub fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		self.game_scene_base.get_scene_base_mut()
	}

	//==============================================================================
	// ViewScaleMode 설정. (엔진을 주면 바로 반영, 아니면 initialize 에서)
	//==============================================================================
	pub fn set_view_scale_mode(&mut self, view_scale_mode: ViewScaleMode, engine: Option<&mut Engine>) {
		self.game_scene_base.set_view_scale_mode(view_scale_mode, engine);
	}

	//==============================================================================
	// 지금 ViewScaleMode 반환.
	//==============================================================================
	pub fn get_view_scale_mode(&self) -> ViewScaleMode {
		self.game_scene_base.get_view_scale_mode()
	}

	//==============================================================================
	// 로딩 화면 가운데 이미지 경로 설정. (create 전에)
	//==============================================================================
	pub fn set_loading_image_url(&mut self, url: &str) {
		self.game_scene_base.set_loading_image_url(url);
	}

	//==============================================================================
	// 로딩 화면 최소 노출 시간 설정. (밀리초, create 전에)
	//==============================================================================
	pub fn set_loading_min_duration_ms(&mut self, milliseconds: f32) {
		self.game_scene_base.set_loading_min_duration_ms(milliseconds);
	}

	//==============================================================================
	// 캔버스 배경색 설정.
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<crate::base::color::Color>) {
		self.game_scene_base.set_scene_background_color(color);
	}

	//==============================================================================
	// 생성. (씬 create 에서 맨 먼저, 원본 load 의 앞부분 + load_assets)
	//==============================================================================
	pub fn create(&mut self, engine: &mut Engine) {
		self.game_scene_base.create(engine);
	}

	//==============================================================================
	// 로딩 화면 출력.
	//==============================================================================
	pub fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.game_scene_base.draw_on_load(engine, graphic);
	}

	//==============================================================================
	// 초기화. (ViewScaleMode, 개발자 도구, 그리고 지금 뷰 크기를 솔버에 반영)
	//==============================================================================
	pub fn initialize(&mut self, engine: &mut Engine) {
		self.game_scene_base.initialize(engine);
		self.layout(engine);
	}

	//==============================================================================
	// 화면 크기 변경됨. (공통 틀의 레이아웃 뒤 솔버 레이아웃)
	//==============================================================================
	pub fn resize(&mut self, engine: &mut Engine, canvas_native_size: Vector2) {
		self.game_scene_base.resize(engine, canvas_native_size);
		self.layout(engine);
	}

	//==============================================================================
	// 레이아웃. (뷰 크기를 솔버에 제안하고, 세이프 에어리어 인셋이 바뀌었을 때만 가이드 제약을 다시 짓고, 풀어서 노드에 옮긴다)
	//==============================================================================
	pub fn layout(&mut self, engine: &Engine) {
		let view_size = engine.get_view_manager().get_view_size();
		self.suggest_screen_size(view_size);
		let safe_area_rect = self.game_scene_base.compute_safe_area_rect(engine);
		let inset_top = safe_area_rect.position.y as f64;
		let inset_left = safe_area_rect.position.x as f64;
		let inset_right = (view_size.x - (safe_area_rect.position.x + safe_area_rect.size.x)).max(0.0) as f64;
		let inset_bottom = (view_size.y - (safe_area_rect.position.y + safe_area_rect.size.y)).max(0.0) as f64;
		let insets = LayoutMargins::create(inset_top, inset_left, inset_bottom, inset_right);
		if insets != self.safe_area_insets {
			let _ = self.set_safe_area_insets(insets);
		}
		self.solve_and_apply();
	}

	//==============================================================================
	// 갱신. (공통 틀 tick, 뷰 크기가 바뀌었으면 레이아웃, 새 UINode 에 솔버를 붙이고 풀어서 노드에 옮긴다. 레이아웃을 다시 했으면 true)
	//==============================================================================
	pub fn tick(&mut self, engine: &mut Engine, time_delta: f32) -> bool {
		let is_layout_changed = self.game_scene_base.tick(engine, time_delta);
		if is_layout_changed {
			self.layout(engine);
		}
		let solver = self.solver.clone();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		UINode::attach_pending_in_tree(node_tree, root_id, &solver);
		self.solve_and_apply();
		is_layout_changed
	}

	//==============================================================================
	// 솔버를 풀고 트리의 UINode 값을 노드에 옮긴다.
	//==============================================================================
	fn solve_and_apply(&mut self) {
		self.solver.update_variables();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		UINode::apply_layout_in_tree(node_tree, root_id);
	}

	//==============================================================================
	// 화면 가이드의 너비와 높이 제안.
	//==============================================================================
	pub fn suggest_screen_size(&mut self, size: Vector2) {
		let _ = self.solver.suggest_value(self.screen_node.get_width_variable(), size.x as f64);
		let _ = self.solver.suggest_value(self.screen_node.get_height_variable(), size.y as f64);
	}

	//==============================================================================
	// 세이프 에어리어 인셋 반환. (UIKit 의 UIView.safeAreaInsets)
	//==============================================================================
	pub fn get_safe_area_insets(&self) -> LayoutMargins {
		self.safe_area_insets
	}

	//==============================================================================
	// 세이프 에어리어 인셋 설정. (가이드 제약 4 개를 바로 다시 짓는다)
	//==============================================================================
	pub fn set_safe_area_insets(&mut self, insets: LayoutMargins) -> Result<(), LayoutError> {
		self.safe_area_insets = insets;
		self.rebuild_safe_area_layout_guide_constraints()
	}

	//==============================================================================
	// 세이프 에어리어 가이드 제약 4 개를 다시 짓는다. (옛 제약을 빼고 지금 인셋으로 지어 다시 건다)
	//==============================================================================
	pub fn rebuild_safe_area_layout_guide_constraints(&mut self) -> Result<(), LayoutError> {
		for constraint in self.safe_area_layout_guide_constraints.iter() {
			if self.solver.has_constraint(constraint) {
				self.solver.remove_constraint(constraint)?;
			}
		}
		let insets = self.safe_area_insets;
		let screen = &self.screen_node;
		let guide = &self.safe_area_layout_guide;
		let guide_left_constraint = guide.left_anchor().equal_to(screen.left_anchor().add(insets.left));
		let guide_top_constraint = guide.top_anchor().equal_to(screen.top_anchor().add(insets.top));
		let guide_right_constraint = guide.right_anchor().equal_to(screen.right_anchor().subtract(insets.right));
		let guide_bottom_constraint = guide.bottom_anchor().equal_to(screen.bottom_anchor().subtract(insets.bottom));
		self.safe_area_layout_guide_constraints = vec![guide_left_constraint, guide_top_constraint, guide_right_constraint, guide_bottom_constraint];
		for constraint in self.safe_area_layout_guide_constraints.iter() {
			self.solver.add_constraint(constraint)?;
		}
		Ok(())
	}

	//==============================================================================
	// 개발자 도구가 입력을 가져가는 중인지.
	//==============================================================================
	pub fn is_dev_tools_capturing_input(&self, engine: &Engine) -> bool {
		self.game_scene_base.is_dev_tools_capturing_input(engine)
	}

	//==============================================================================
	// 이전 출력. (캔버스 전체를 배경색으로, 세이프 에어리어 바깥 포함)
	//==============================================================================
	pub fn pre_draw(&self, engine: &Engine, graphic: &mut Graphic) {
		self.game_scene_base.pre_draw(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (개발자 도구를 화면 위에)
	//==============================================================================
	pub fn post_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.game_scene_base.post_draw(engine, graphic);
	}

	//==============================================================================
	// 솔버 반환. (값싼 핸들이라 복제해 써도 같은 솔버다)
	//==============================================================================
	pub fn get_solver(&self) -> &LayoutSolver {
		&self.solver
	}

	//==============================================================================
	// 화면 가이드 UINode 반환. (트리에 들지 않는다. left, top 은 0, width, height 는 뷰 크기)
	//==============================================================================
	pub fn get_screen_node(&self) -> &UINode {
		&self.screen_node
	}

	//==============================================================================
	// 세이프 에어리어 가이드 UINode 반환. (화면 가이드에서 인셋만큼 안쪽)
	//==============================================================================
	pub fn get_safe_area_layout_guide(&self) -> &UINode {
		&self.safe_area_layout_guide
	}
}

impl Default for UISceneBase {
	fn default() -> UISceneBase {
		UISceneBase::new()
	}
}

//==============================================================================
// UI 씬. (원본 UIScene 을 상속한 씬이 덮어쓰는 자리, GameScene 과 같은 규칙)
// - ui_scene! 이 만든 Scene 함수는 바탕(UISceneBase)을 먼저 부르고 이 트레이트의 같은 이름 함수를 부른다. post_draw 만 씬의 것이 먼저다.
//==============================================================================
pub trait UIScene: 'static {
	//==============================================================================
	// 바탕 반환.
	//==============================================================================
	fn get_ui_scene_base(&self) -> &UISceneBase;

	//==============================================================================
	// 바탕 반환. (변경용)
	//==============================================================================
	fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase;

	//==============================================================================
	// 씬의 애셋 읽기. (원본 async load_assets 덮어쓰기)
	// - await 로 읽으려면 engine.spawn_scene_task(self, ...) 로 불러오기 작업을 맡기고 씬 공통 상태의 track_task 로 맡긴다.
	//==============================================================================
	fn load_assets(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 로딩 화면 위에 더 그리기.
	//==============================================================================
	fn draw_on_load(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 초기화. (바탕 initialize 뒤)
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 소멸.
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 레이아웃. (바탕이 솔버 레이아웃을 한 뒤)
	//==============================================================================
	fn layout(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 갱신. (바탕 tick 뒤)
	//==============================================================================
	fn tick(&mut self, _engine: &mut Engine, _time_delta: f32) {
	}

	//==============================================================================
	// 이전 출력. (바탕 뒤)
	//==============================================================================
	fn pre_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 출력. (노드 트리를 그린 뒤)
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 이후 출력. (바탕의 개발자 도구보다 먼저)
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	// 종료처리. (내릴 때 unload 앞, 원본 Scene.finalize 덮어쓰기)
	//==============================================================================
	fn finalize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 로딩 해제. (내릴 때 finalize 뒤, 원본 Scene.unload 덮어쓰기)
	//==============================================================================
	fn unload(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 페이지 프로세스 복원과 가시성 복원. (원본 onPageProcessRestored, onPageVisibilityRestored 덮어쓰기)
	//==============================================================================
	fn on_page_process_restored(&mut self, _engine: &mut Engine) {
	}

	fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	// 씬의 터치 훅. (원본 touchPress, touchMove, touchRelease, touchCancel, touchWheel 덮어쓰기, 바탕이 UI 와 놀이 입력에 넘긴 뒤 엔진이 부른다)
	//==============================================================================
	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	fn touch_move(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	fn touch_release(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	fn touch_cancel(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	fn touch_wheel(&mut self, _engine: &mut Engine, _view_input_position: Vector2, _wheel_delta: Vector2) {
	}
}

//==============================================================================
// UIScene 을 구현한 씬에 Scene 을 만들어 준다. (vanilla::ui_scene!(MenuScene);)
//==============================================================================
#[macro_export]
macro_rules! ui_scene {
	($scene_type:ty) => {
		impl $crate::core::scene::Scene for $scene_type {
			fn get_scene_base(&self) -> &$crate::core::scene::SceneBase {
				$crate::ui::uiscene::UIScene::get_ui_scene_base(self).get_scene_base()
			}

			fn get_scene_base_mut(&mut self) -> &mut $crate::core::scene::SceneBase {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).get_scene_base_mut()
			}

			fn create(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).create(engine);
				$crate::ui::uiscene::UIScene::load_assets(self, engine);
			}

			fn draw_on_load(&mut self, engine: &mut $crate::core::engine::Engine, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).draw_on_load(engine, graphic);
				$crate::ui::uiscene::UIScene::draw_on_load(self, engine, graphic);
			}

			fn initialize(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).initialize(engine);
				$crate::ui::uiscene::UIScene::initialize(self, engine);
			}

			fn destroy(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::destroy(self, engine);
			}

			fn finalize(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::finalize(self, engine);
			}

			fn unload(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::unload(self, engine);
			}

			fn on_page_process_restored(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::on_page_process_restored(self, engine);
			}

			fn on_page_visibility_restored(&mut self, engine: &mut $crate::core::engine::Engine) {
				$crate::ui::uiscene::UIScene::on_page_visibility_restored(self, engine);
			}

			fn touch_press(&mut self, engine: &mut $crate::core::engine::Engine, view_input_position: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::touch_press(self, engine, view_input_position);
			}

			fn touch_move(&mut self, engine: &mut $crate::core::engine::Engine, view_input_position: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::touch_move(self, engine, view_input_position);
			}

			fn touch_release(&mut self, engine: &mut $crate::core::engine::Engine, view_input_position: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::touch_release(self, engine, view_input_position);
			}

			fn touch_cancel(&mut self, engine: &mut $crate::core::engine::Engine, view_input_position: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::touch_cancel(self, engine, view_input_position);
			}

			fn touch_wheel(&mut self, engine: &mut $crate::core::engine::Engine, view_input_position: $crate::base::vector2::Vector2, wheel_delta: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::touch_wheel(self, engine, view_input_position, wheel_delta);
			}

			fn resize(&mut self, engine: &mut $crate::core::engine::Engine, canvas_native_size: $crate::base::vector2::Vector2) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).resize(engine, canvas_native_size);
				$crate::ui::uiscene::UIScene::layout(self, engine);
			}

			fn tick(&mut self, engine: &mut $crate::core::engine::Engine, time_delta: f32) {
				if $crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).tick(engine, time_delta) {
					$crate::ui::uiscene::UIScene::layout(self, engine);
				}
				if let Some(screen_fade_dark_starter) = $crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).get_game_scene_base_mut().take_screen_fade_dark_starter() {
					screen_fade_dark_starter.start(self, engine);
				}
				$crate::ui::uiscene::UIScene::tick(self, engine, time_delta);
			}

			fn pre_draw(&mut self, engine: &mut $crate::core::engine::Engine, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base(self).pre_draw(engine, graphic);
				$crate::ui::uiscene::UIScene::pre_draw(self, engine, graphic);
			}

			fn draw(&mut self, engine: &mut $crate::core::engine::Engine, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).get_scene_base_mut().draw(graphic);
				$crate::ui::uiscene::UIScene::draw(self, engine, graphic);
			}

			fn post_draw(&mut self, engine: &mut $crate::core::engine::Engine, graphic: &mut $crate::core::graphic::Graphic) {
				$crate::ui::uiscene::UIScene::post_draw(self, engine, graphic);
				$crate::ui::uiscene::UIScene::get_ui_scene_base_mut(self).post_draw(engine, graphic);
			}
		}
	};
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;
	use crate::core::viewmanager::SafeAreaInsets;

	//==============================================================================
	// 트리에 붙인 UINode 는 tick 에 솔버가 붙고, 세이프 에어리어 가이드에 맞춰 놓인다. 뷰 크기가 바뀌면 다시 놓인다.
	//==============================================================================
	#[test]
	fn ui_nodes_follow_safe_area_guide() {
		let mut engine = Engine::new(EngineConfiguration::new());
		engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
		engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 20.0, 10.0)));
		let mut ui_scene_base = UISceneBase::new();
		ui_scene_base.initialize(&mut engine);
		let guide_left = ui_scene_base.get_safe_area_layout_guide().left_anchor();
		let guide_top = ui_scene_base.get_safe_area_layout_guide().top_anchor();
		let guide_right = ui_scene_base.get_safe_area_layout_guide().right_anchor();
		let node_tree = ui_scene_base.get_scene_base_mut().get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		let bar_id = node_tree.create_child(root_id, "bar");
		node_tree.add_component(bar_id, UINode::new());
		node_tree.with_component_mut::<UINode, _>(bar_id, |bar, _tree| {
			bar.add_constraint(&bar.left_anchor().equal_to(guide_left.clone())).expect("left");
			bar.add_constraint(&bar.top_anchor().equal_to(guide_top.clone())).expect("top");
			bar.add_constraint(&bar.right_anchor().equal_to(guide_right.clone())).expect("right");
			bar.add_constraint(&bar.height_anchor().equal_to(50)).expect("height");
		});
		assert!(ui_scene_base.get_scene_base().get_node_tree().find_child_by_name(root_id, "safeArea").is_none());
		ui_scene_base.tick(&mut engine, 0.016);
		let bar_node = &ui_scene_base.get_scene_base().get_node_tree()[bar_id];
		assert_eq!(bar_node.get_local_position(), Vector2::create(10.0, 40.0));
		assert_eq!(bar_node.get_content_size(), Vector2::create(790.0, 50.0));
		assert_eq!(ui_scene_base.get_safe_area_insets(), LayoutMargins::create(40.0, 10.0, 20.0, 0.0));

		engine.get_view_manager_mut().calculate_view_rect(1000, 600, 1.0);
		assert!(ui_scene_base.tick(&mut engine, 0.016));
		let bar_node = &ui_scene_base.get_scene_base().get_node_tree()[bar_id];
		assert_eq!(bar_node.get_content_size(), Vector2::create(990.0, 50.0));
		assert!((ui_scene_base.get_screen_node().get_width() - 1000.0).abs() < 0.001);
	}
}
