//==============================================================================
//! UI 씬. (원본 ui/uiscene.js 의 UIScene)
//! - GameScene 과 같은 공통 틀(개발자 도구, 터치 라우팅, 로딩 화면, 세이프 에어리어 인셋, 뷰 크기 변화 감지, 캔버스 배경 칠하기)을 가진 씬이다.
//! - 제 LayoutSolver 를 들고 tick 마다 트리의 UINode 에 솔버를 붙이고(attach_pending_in_tree) 한 번 푼 뒤 노드에 옮긴다.
//! - 화면 가이드 UINode(screen_node): 왼쪽 위 (0, 0) 고정, 너비와 높이는 편집 변수라 크기가 바뀔 때 suggest_value 로 고친다. (UIKit 의 UIWindow.bounds)
//! - 세이프 에어리어 가이드 UINode(safe_area_layout_guide): 화면 가이드에서 세이프 에어리어 인셋만큼 안쪽. 레이아웃 때 인셋을 스스로 맞춘다.
//!   (UIKit 의 UIView.safeAreaLayoutGuide)
//! - 러스트 판은 상속 대신 씬이 UISceneBase 를 품고 UIScene 트레이트를 구현한 뒤 ui_scene! 한 줄로 Scene 을 얻는다. (GameScene, game_scene! 과 같은 꼴)
//!   ```text
//!     struct MenuScene { ui_scene_base: UISceneBase }
//!     impl UIScene for MenuScene {
//!         fn get_ui_scene_base(&self) -> &UISceneBase { &self.ui_scene_base }
//!         fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase { &mut self.ui_scene_base }
//!         fn initialize(&mut self, engine: &mut Engine) { /* UINode 를 붙인 노드를 만들고 get_safe_area_layout_guide 의 앵커로 제약을 건다 */ }
//!     }
//!     vanilla::ui_scene!(MenuScene);
//!   ```
//! - 원본과 다른 것:
//!   - 원본은 Scene 을 바로 상속해 GameScene 과 같은 코드를 따로 든다. 러스트 판은 공통 틀을 GameSceneBase 로 품고(세이프 에어리어 노드는 만들지 않는다)
//!     솔버와 두 가이드만 더한다. 그래서 GameSceneBase 의 화면 페이드와 놀이 입력도 쓸 수 있다. (원본 UIScene 에는 없다)
//!   - 원본은 create 에서 솔버와 가이드를 만든다. 러스트 판은 엔진이 필요 없어 생성할 때 만든다. 원본 load 끝의 "뷰 크기를 솔버에 바로 반영"은 initialize 에서 한다.
//!   - 세이프 에어리어 인셋은 { top, left, bottom, right } 객체 대신 LayoutMargins 다. 솔버 오류는 `Result<(), LayoutError>` 로 돌려준다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::engine::Engine;
use crate::core::graphic::Graphic;
use crate::core::scene::SceneBase;
use crate::core::viewmanager::ViewScaleMode;
use crate::game::gamescene::GameSceneBase;
use crate::misc::devtools::DevTools;
use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
use crate::ui::autolayout::layoutsolver::{LayoutError, LayoutSolver};
use crate::ui::autolayout::layoutstrength::LayoutStrength;
use crate::ui::uinode::{LayoutMargins, UINode};

//==============================================================================
/// UI 씬의 바탕.
/// - GameSceneBase 를 품고 솔버, 화면 가이드, 세이프 에어리어 가이드를 더한다. UIScene 을 구현한 씬이 필드로 품는다.
///
/// # 예제
/// 세이프 에어리어 가이드에 붙인 머리 띠가 tick 에 놓인다
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, SafeAreaInsets, UINode, UISceneBase, Vector2};
///
/// let mut engine = Engine::new(EngineConfiguration::new());
/// engine.get_view_manager_mut().calculate_view_rect(800, 600, 1.0);
/// engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 20.0, 10.0)));
/// let mut ui_scene_base = UISceneBase::new();
/// ui_scene_base.initialize(&mut engine);
///
/// let guide_left = ui_scene_base.get_safe_area_layout_guide().left_anchor();
/// let guide_top = ui_scene_base.get_safe_area_layout_guide().top_anchor();
/// let guide_right = ui_scene_base.get_safe_area_layout_guide().right_anchor();
/// let node_tree = ui_scene_base.get_scene_base_mut().get_node_tree_mut();
/// let root_id = node_tree.get_root_id();
/// let bar_id = node_tree.create_child(root_id, "bar");
/// node_tree.add_component(bar_id, UINode::new());
/// node_tree.with_component_mut::<UINode, _>(bar_id, |bar, _tree| {
/// 	bar.add_constraint(&bar.left_anchor().equal_to(guide_left.clone())).unwrap();
/// 	bar.add_constraint(&bar.top_anchor().equal_to(guide_top.clone())).unwrap();
/// 	bar.add_constraint(&bar.right_anchor().equal_to(guide_right.clone())).unwrap();
/// 	bar.add_constraint(&bar.height_anchor().equal_to(50)).unwrap();
/// });
///
/// ui_scene_base.tick(&mut engine, 0.016);
/// let bar_node = &ui_scene_base.get_scene_base().get_node_tree()[bar_id];
/// assert_eq!(bar_node.get_local_position(), Vector2::create(10.0, 40.0));
/// assert_eq!(bar_node.get_content_size(), Vector2::create(790.0, 50.0));
/// ```
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
	/// 생성. (솔버, 화면 가이드, 세이프 에어리어 가이드. 인셋 기본 0 이라 두 가이드가 같은 영역)
	///
	/// # 돌려줌
	/// 새 바탕. (화면 가이드는 왼쪽 위 (0, 0) 고정, 너비와 높이는 strong 강도의 편집 변수)
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
	/// 공통 틀 반환. (로딩 화면, 배경색, 페이드 같은 GameSceneBase 기능)
	///
	/// # 돌려줌
	/// 품은 GameSceneBase 의 참조.
	//==============================================================================
	pub fn get_game_scene_base(&self) -> &GameSceneBase {
		&self.game_scene_base
	}

	//==============================================================================
	/// 공통 틀 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 GameSceneBase 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
		&mut self.game_scene_base
	}

	//==============================================================================
	/// 씬 공통 상태 반환.
	///
	/// # 돌려줌
	/// 씬 공통 상태(노드 트리, 트윈 등)의 참조.
	//==============================================================================
	pub fn get_scene_base(&self) -> &SceneBase {
		self.game_scene_base.get_scene_base()
	}

	//==============================================================================
	/// 씬 공통 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 씬 공통 상태의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		self.game_scene_base.get_scene_base_mut()
	}

	//==============================================================================
	/// ViewScaleMode 설정. (엔진을 주면 바로 반영, 아니면 initialize 에서)
	///
	/// # 인자
	/// - `view_scale_mode`: 뷰 배율 방식.
	/// - `engine`: 바로 반영할 엔진. None 이면 initialize 에서 적용한다.
	//==============================================================================
	pub fn set_view_scale_mode(&mut self, view_scale_mode: ViewScaleMode, engine: Option<&mut Engine>) {
		self.game_scene_base.set_view_scale_mode(view_scale_mode, engine);
	}

	//==============================================================================
	/// 지금 ViewScaleMode 반환.
	///
	/// # 돌려줌
	/// 지금 ViewScaleMode. (기본 None)
	//==============================================================================
	pub fn get_view_scale_mode(&self) -> ViewScaleMode {
		self.game_scene_base.get_view_scale_mode()
	}

	//==============================================================================
	/// 로딩 화면 가운데 이미지 경로 설정. (create 전에)
	///
	/// # 인자
	/// - `url`: 로딩 화면 가운데 이미지 경로. (빈 글이면 이미지 없이 게이지만)
	//==============================================================================
	pub fn set_loading_image_url(&mut self, url: &str) {
		self.game_scene_base.set_loading_image_url(url);
	}

	//==============================================================================
	/// 로딩 화면 최소 노출 시간 설정. (밀리초, create 전에)
	///
	/// # 인자
	/// - `milliseconds`: 로딩 화면을 보일 가장 짧은 시간. (밀리초, 기본 3000)
	//==============================================================================
	pub fn set_loading_min_duration_ms(&mut self, milliseconds: f64) {
		self.game_scene_base.set_loading_min_duration_ms(milliseconds);
	}

	//==============================================================================
	/// 캔버스 배경색 설정.
	///
	/// # 인자
	/// - `color`: 캔버스 배경색. (Color 나 "#rrggbb" 같은 글)
	//==============================================================================
	pub fn set_scene_background_color(&mut self, color: impl Into<crate::base::color::Color>) {
		self.game_scene_base.set_scene_background_color(color);
	}

	//==============================================================================
	/// 생성. (씬 create 에서 맨 먼저, 원본 load 의 앞부분 + load_assets)
	///
	/// # 인자
	/// - `engine`: 엔진.
	//==============================================================================
	pub fn create(&mut self, engine: &mut Engine) {
		self.game_scene_base.create(engine);
	}

	//==============================================================================
	/// 로딩 화면 출력.
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.game_scene_base.draw_on_load(engine, graphic);
	}

	//==============================================================================
	/// 초기화. (ViewScaleMode, 개발자 도구, 그리고 지금 뷰 크기를 솔버에 반영)
	///
	/// # 인자
	/// - `engine`: 엔진. (뷰 크기와 세이프 에어리어를 읽는다)
	//==============================================================================
	pub fn initialize(&mut self, engine: &mut Engine) {
		self.game_scene_base.initialize(engine);
		self.layout(engine);
	}

	//==============================================================================
	/// 화면 크기 변경됨. (공통 틀의 레이아웃 뒤 솔버 레이아웃)
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `canvas_native_size`: 바뀐 캔버스 크기. (공통 틀에 넘기고, 솔버 레이아웃은 이 값 대신 뷰 매니저의 뷰 크기를 읽는다)
	//==============================================================================
	pub fn resize(&mut self, engine: &mut Engine, canvas_native_size: Vector2) {
		self.game_scene_base.resize(engine, canvas_native_size);
		self.layout(engine);
	}

	//==============================================================================
	/// 레이아웃. (뷰 크기를 솔버에 제안하고, 세이프 에어리어 인셋이 바뀌었을 때만 가이드 제약을 다시 짓고, 풀어서 노드에 옮긴다)
	///
	/// # 인자
	/// - `engine`: 뷰 크기와 세이프 에어리어 영역을 읽을 엔진.
	//==============================================================================
	pub fn layout(&mut self, engine: &Engine) {
		let view_size = engine.get_view_manager().get_view_size();
		self.suggest_screen_size(view_size);
		let safe_area_rect = self.game_scene_base.compute_safe_area_rect(engine);
		let inset_top = safe_area_rect.position.y;
		let inset_left = safe_area_rect.position.x;
		let inset_right = (view_size.x - (safe_area_rect.position.x + safe_area_rect.size.x)).max(0.0);
		let inset_bottom = (view_size.y - (safe_area_rect.position.y + safe_area_rect.size.y)).max(0.0);
		let insets = LayoutMargins::create(inset_top, inset_left, inset_bottom, inset_right);
		if insets != self.safe_area_insets {
			let _ = self.set_safe_area_insets(insets);
		}
		self.solve_and_apply();
	}

	//==============================================================================
	/// 갱신. (공통 틀 tick, 뷰 크기가 바뀌었으면 레이아웃, 새 UINode 에 솔버를 붙이고 풀어서 노드에 옮긴다. 레이아웃을 다시 했으면 true)
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `time_delta`: 지난 프레임부터 흐른 시간. (초)
	///
	/// # 돌려줌
	/// 뷰 크기가 바뀌어 레이아웃을 다시 했으면 true.
	//==============================================================================
	pub fn tick(&mut self, engine: &mut Engine, time_delta: f64) -> bool {
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
	/// 솔버를 풀고 트리의 UINode 값을 노드에 옮긴다.
	//==============================================================================
	fn solve_and_apply(&mut self) {
		self.solver.update_variables();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		let root_id = node_tree.get_root_id();
		UINode::apply_layout_in_tree(node_tree, root_id);
	}

	//==============================================================================
	/// 화면 가이드의 너비와 높이 제안.
	///
	/// # 인자
	/// - `size`: 화면 가이드에 줄 너비와 높이. (뷰 좌표, 솔버 오류는 버린다. 노드 값은 솔버를 푼 뒤에 바뀐다)
	///
	/// # 예제
	/// 엔진 없이 화면 크기를 주고 가이드 읽기
	/// ```rust
	/// use vanilla::{UISceneBase, Vector2};
	///
	/// let mut ui_scene_base = UISceneBase::new();
	/// ui_scene_base.suggest_screen_size(Vector2::create(1280.0, 720.0));
	/// ui_scene_base.get_solver().update_variables();
	/// assert!((ui_scene_base.get_screen_node().get_width() - 1280.0).abs() < 1e-6);
	/// assert!((ui_scene_base.get_safe_area_layout_guide().get_bottom() - 720.0).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub fn suggest_screen_size(&mut self, size: Vector2) {
		let _ = self.solver.suggest_value(self.screen_node.get_width_variable(), size.x);
		let _ = self.solver.suggest_value(self.screen_node.get_height_variable(), size.y);
	}

	//==============================================================================
	/// 세이프 에어리어 인셋 반환. (UIKit 의 UIView.safeAreaInsets)
	///
	/// # 돌려줌
	/// 지금 세이프 에어리어 인셋. (뷰 좌표, 처음은 모두 0)
	//==============================================================================
	pub fn get_safe_area_insets(&self) -> LayoutMargins {
		self.safe_area_insets
	}

	//==============================================================================
	/// 세이프 에어리어 인셋 설정. (가이드 제약 4 개를 바로 다시 짓는다)
	///
	/// # 인자
	/// - `insets`: 새 인셋. (뷰 좌표, layout 이 엔진에서 읽은 값과 다르면 그 값으로 다시 바꾼다)
	///
	/// # 돌려줌
	/// Ok(()). 가이드 제약을 빼거나 다시 걸다 실패하면 그 LayoutError.
	///
	/// # 예제
	/// 위 40, 아래 20 인셋으로 가이드를 줄이기
	/// ```rust
	/// use vanilla::{LayoutMargins, UISceneBase, Vector2};
	///
	/// let mut ui_scene_base = UISceneBase::new();
	/// ui_scene_base.suggest_screen_size(Vector2::create(400.0, 700.0));
	/// ui_scene_base.set_safe_area_insets(LayoutMargins::create(40.0, 0.0, 20.0, 0.0)).unwrap();
	/// ui_scene_base.get_solver().update_variables();
	/// let guide = ui_scene_base.get_safe_area_layout_guide();
	/// assert!((guide.get_top() - 40.0).abs() < 1e-6);
	/// assert!((guide.get_height() - 640.0).abs() < 1e-6);
	/// ```
	//==============================================================================
	pub fn set_safe_area_insets(&mut self, insets: LayoutMargins) -> Result<(), LayoutError> {
		self.safe_area_insets = insets;
		self.rebuild_safe_area_layout_guide_constraints()
	}

	//==============================================================================
	/// 세이프 에어리어 가이드 제약 4 개를 다시 짓는다. (옛 제약을 빼고 지금 인셋으로 지어 다시 건다)
	///
	/// # 돌려줌
	/// Ok(()). 솔버에서 옛 제약을 빼거나 새 제약을 걸다 실패하면 그 LayoutError.
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
	/// 개발자 도구 반환. (원본 UIScene.getDevTools, initialize 전이거나 EngineConfiguration::use_dev_tools 가 꺼져 있으면 None)
	///
	/// # 돌려줌
	/// 개발자 도구. initialize 전이거나 꺼져 있으면 None.
	///
	/// # 예제
	/// 개발자 도구를 켜지 않은 바탕
	/// ```rust
	/// use vanilla::UISceneBase;
	///
	/// let ui_scene_base = UISceneBase::new();
	/// assert!(ui_scene_base.get_dev_tools().is_none());
	/// ```
	//==============================================================================
	pub fn get_dev_tools(&self) -> Option<&DevTools> {
		self.game_scene_base.get_dev_tools()
	}

	//==============================================================================
	/// 개발자 도구 반환. (변경용)
	///
	/// # 돌려줌
	/// 개발자 도구. (변경용) initialize 전이거나 꺼져 있으면 None.
	//==============================================================================
	pub fn get_dev_tools_mut(&mut self) -> Option<&mut DevTools> {
		self.game_scene_base.get_dev_tools_mut()
	}

	//==============================================================================
	/// 개발자 도구가 입력을 가져가는 중인지.
	///
	/// # 인자
	/// - `engine`: 누른 자리를 읽을 엔진.
	///
	/// # 돌려줌
	/// 개발자 도구가 보이고 누른 자리가 그 판 안이면 true. (개발자 도구가 꺼져 있으면 false)
	//==============================================================================
	pub fn is_dev_tools_capturing_input(&self, engine: &Engine) -> bool {
		self.game_scene_base.is_dev_tools_capturing_input(engine)
	}

	//==============================================================================
	/// 이전 출력. (캔버스 전체를 배경색으로, 세이프 에어리어 바깥 포함)
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn pre_draw(&self, engine: &Engine, graphic: &mut Graphic) {
		self.game_scene_base.pre_draw(engine, graphic);
	}

	//==============================================================================
	/// 이후 출력. (개발자 도구를 화면 위에)
	///
	/// # 인자
	/// - `engine`: 엔진.
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn post_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.game_scene_base.post_draw(engine, graphic);
	}

	//==============================================================================
	/// 솔버 반환. (값싼 핸들이라 복제해 써도 같은 솔버다)
	///
	/// # 돌려줌
	/// 씬 솔버의 참조.
	//==============================================================================
	pub fn get_solver(&self) -> &LayoutSolver {
		&self.solver
	}

	//==============================================================================
	/// 화면 가이드 UINode 반환. (트리에 들지 않는다. left, top 은 0, width, height 는 뷰 크기)
	///
	/// # 돌려줌
	/// 화면 가이드 UINode 의 참조.
	//==============================================================================
	pub fn get_screen_node(&self) -> &UINode {
		&self.screen_node
	}

	//==============================================================================
	/// 세이프 에어리어 가이드 UINode 반환. (화면 가이드에서 인셋만큼 안쪽)
	///
	/// # 돌려줌
	/// 세이프 에어리어 가이드 UINode 의 참조. (앵커를 읽어 노드 제약에 쓴다)
	//==============================================================================
	pub fn get_safe_area_layout_guide(&self) -> &UINode {
		&self.safe_area_layout_guide
	}
}

impl Default for UISceneBase {
	//==============================================================================
	/// 생성. (UISceneBase::new 와 같다)
	//==============================================================================
	fn default() -> UISceneBase {
		UISceneBase::new()
	}
}

//==============================================================================
/// UI 씬. (원본 UIScene 을 상속한 씬이 덮어쓰는 자리, GameScene 과 같은 규칙)
/// - ui_scene! 이 만든 Scene 함수는 바탕(UISceneBase)을 먼저 부르고 이 트레이트의 같은 이름 함수를 부른다. post_draw 만 씬의 것이 먼저다.
///
/// # 예제
/// 세이프 에어리어 안쪽 왼쪽 위에 제목 칸을 놓는 씬
/// ```rust,no_run
/// use vanilla::{Engine, EngineConfiguration, UINode, UIScene, UISceneBase};
///
/// struct MenuScene {
/// 	ui_scene_base: UISceneBase,
/// }
///
/// impl UIScene for MenuScene {
/// 	fn get_ui_scene_base(&self) -> &UISceneBase {
/// 		&self.ui_scene_base
/// 	}
///
/// 	fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase {
/// 		&mut self.ui_scene_base
/// 	}
///
/// 	fn initialize(&mut self, _engine: &mut Engine) {
/// 		let guide_left = self.ui_scene_base.get_safe_area_layout_guide().left_anchor();
/// 		let guide_top = self.ui_scene_base.get_safe_area_layout_guide().top_anchor();
/// 		let node_tree = self.ui_scene_base.get_scene_base_mut().get_node_tree_mut();
/// 		let root_id = node_tree.get_root_id();
/// 		let title_id = node_tree.create_child(root_id, "title");
/// 		node_tree.add_component(title_id, UINode::new());
/// 		node_tree.with_component_mut::<UINode, _>(title_id, |title, _tree| {
/// 			title.add_constraint(&title.left_anchor().equal_to(guide_left.add(16))).unwrap();
/// 			title.add_constraint(&title.top_anchor().equal_to(guide_top.add(16))).unwrap();
/// 			title.add_constraint(&title.width_anchor().equal_to(320)).unwrap();
/// 			title.add_constraint(&title.height_anchor().equal_to(48)).unwrap();
/// 		});
/// 	}
/// }
///
/// vanilla::ui_scene!(MenuScene);
///
/// fn main() {
/// 	let engine = Engine::new(EngineConfiguration::new());
/// 	let menu_scene = MenuScene {
/// 		ui_scene_base: UISceneBase::new(),
/// 	};
/// 	if let Err(error) = engine.run(Box::new(menu_scene)) {
/// 		eprintln!("{}", error);
/// 	}
/// }
/// ```
//==============================================================================
pub trait UIScene: 'static {
	//==============================================================================
	/// 바탕 반환.
	///
	/// # 돌려줌
	/// 씬이 품은 UISceneBase 의 참조.
	//==============================================================================
	fn get_ui_scene_base(&self) -> &UISceneBase;

	//==============================================================================
	/// 바탕 반환. (변경용)
	///
	/// # 돌려줌
	/// 씬이 품은 UISceneBase 의 바꿀 수 있는 참조.
	//==============================================================================
	fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase;

	//==============================================================================
	/// 씬의 애셋 읽기. (원본 async load_assets 덮어쓰기)
	/// - await 로 읽으려면 engine.spawn_scene_task(self, ...) 로 불러오기 작업을 맡기고 씬 공통 상태의 track_task 로 맡긴다.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn load_assets(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 로딩 화면 위에 더 그리기.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn draw_on_load(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 초기화. (바탕 initialize 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 소멸.
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn destroy(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 레이아웃. (바탕이 솔버 레이아웃을 한 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn layout(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 갱신. (바탕 tick 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_time_delta`: 지난 프레임부터 흐른 시간. (초)
	//==============================================================================
	fn tick(&mut self, _engine: &mut Engine, _time_delta: f64) {
	}

	//==============================================================================
	/// 이전 출력. (바탕 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn pre_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 출력. (노드 트리를 그린 뒤)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 이후 출력. (바탕의 개발자 도구보다 먼저)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_graphic`: 그릴 그래픽.
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, _graphic: &mut Graphic) {
	}

	//==============================================================================
	/// 종료처리. (내릴 때 unload 앞, 원본 Scene.finalize 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn finalize(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 로딩 해제. (내릴 때 finalize 뒤, 원본 Scene.unload 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn unload(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 페이지 프로세스 복원과 가시성 복원. (원본 onPageProcessRestored, onPageVisibilityRestored 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn on_page_process_restored(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 페이지 가시성 복원. (원본 onPageVisibilityRestored 덮어쓰기)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	//==============================================================================
	fn on_page_visibility_restored(&mut self, _engine: &mut Engine) {
	}

	//==============================================================================
	/// 씬의 터치 훅. (원본 touchPress, touchMove, touchRelease, touchCancel, touchWheel 덮어쓰기, 바탕이 UI 와 놀이 입력에 넘긴 뒤 엔진이 부른다)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 누른 자리. (뷰 좌표)
	//==============================================================================
	fn touch_press(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 터치 이동. (누르고 있는 채 움직였을 때, 누른 프레임은 빼고)
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
	/// - `_view_input_position`: 마지막 자리. (뷰 좌표)
	//==============================================================================
	fn touch_cancel(&mut self, _engine: &mut Engine, _view_input_position: Vector2) {
	}

	//==============================================================================
	/// 마우스 휠. (이번 프레임에 쌓인 휠이 있을 때)
	///
	/// # 인자
	/// - `_engine`: 엔진.
	/// - `_view_input_position`: 포인터 자리. (뷰 좌표)
	/// - `_wheel_delta`: 이번 프레임에 쌓인 휠 이동량. (픽셀, x 는 오른쪽으로, y 는 내용을 아래로 넘기면 양수)
	//==============================================================================
	fn touch_wheel(&mut self, _engine: &mut Engine, _view_input_position: Vector2, _wheel_delta: Vector2) {
	}
}

//==============================================================================
/// UIScene 을 구현한 씬에 Scene 을 만들어 준다. (vanilla::ui_scene!(MenuScene);)
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

			fn tick(&mut self, engine: &mut $crate::core::engine::Engine, time_delta: f64) {
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
	/// 트리에 붙인 UINode 는 tick 에 솔버가 붙고, 세이프 에어리어 가이드에 맞춰 놓인다. 뷰 크기가 바뀌면 다시 놓인다.
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
