//==============================================================================
// UI 씬과 오토레이아웃 예제.
// - UIScene 트레이트 + ui_scene! 으로 씬을 만들고, 노드마다 UINode 를 붙여 제약으로 놓는다. (원본 UIScene, UINode, LayoutSolver)
// - 머리 띠: 세이프 에어리어 가이드의 왼쪽, 위, 오른쪽에 붙고 높이 72. 제목 글은 띠 가운데, 크기는 글의 자연 크기(intrinsic content size)다.
// - 옆 판: 머리 띠 아래부터 바닥까지, 너비는 화면의 25% 를 바라되(약한 제약) 200 ~ 320 사이. 본문: 옆 판 오른쪽의 나머지, 여백 가이드 안쪽에 단추 셋을 같은 너비로.
//   단추는 가로 hug 우선순위를 낮춰(FITTING_SIZE_LEVEL) 글의 자연 너비로 줄어들려는 힘이 옆 판을 밀지 않게 한다.
// - 세이프 에어리어는 위 40, 아래 30 을 흉내 낸다. 창 크기를 바꾸면 제약대로 다시 놓인다.
// - VANILLA_EXAMPLE_DEVTOOLS=1 이면 개발자 도구(F2)를 처음부터 연다.
// - 실행: cargo run --example uiscenelayout
//==============================================================================

use vanilla::{
	Color, Engine, EngineConfiguration, LayoutConstraint, LayoutConstraintAxis, LayoutMargins, LayoutPriority, LayoutStrength, NodeId, NodeTree, Paint, SafeAreaInsets, UILabel, UINode, UIScene, UISceneBase, Vector2,
};

//==============================================================================
// 첫 씬.
//==============================================================================
struct LayoutScene {
	ui_scene_base: UISceneBase,
	title_id: Option<NodeId>,
	elapsed_seconds: f64,
	is_title_changed: bool,
}

impl LayoutScene {
	//==============================================================================
	// UINode 와 Paint 를 붙인 노드 만들기.
	//==============================================================================
	fn create_ui_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, color: &str) -> NodeId {
		let node_id = node_tree.create_child(parent_id, name);
		node_tree.add_component(node_id, UINode::new());
		if !color.is_empty() {
			let paint = node_tree[node_id].add_component(Paint::create_with_color(color));
			paint.set_round_size(10.0);
		}
		node_id
	}

	//==============================================================================
	// 노드의 UINode 에 제약 걸기. (제약 짓는 함수가 그 UINode 를 받는다)
	//==============================================================================
	fn constrain(node_tree: &mut NodeTree, node_id: NodeId, build_constraints: impl FnOnce(&UINode) -> Vec<LayoutConstraint>) {
		node_tree.with_component_mut::<UINode, _>(node_id, |ui_node, _tree| {
			for constraint in build_constraints(ui_node) {
				ui_node.add_constraint(&constraint).expect("constraint");
			}
		});
	}

	//==============================================================================
	// 노드의 UINode 반환. (앵커를 읽을 때)
	//==============================================================================
	fn read_ui_node(node_tree: &NodeTree, node_id: NodeId) -> &UINode {
		node_tree[node_id].get_component::<UINode>().expect("ui node")
	}
}

impl UIScene for LayoutScene {
	fn get_ui_scene_base(&self) -> &UISceneBase {
		&self.ui_scene_base
	}

	fn get_ui_scene_base_mut(&mut self) -> &mut UISceneBase {
		&mut self.ui_scene_base
	}

	//==============================================================================
	// 애셋 읽기 자리. (세이프 에어리어를 흉내 낸다)
	//==============================================================================
	fn load_assets(&mut self, engine: &Engine) {
		engine.get_view_manager_mut().set_safe_area_override(Some(SafeAreaInsets::create(40.0, 0.0, 30.0, 0.0)));
	}

	//==============================================================================
	// 초기화. (노드를 짓고 제약을 건다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let screen = self.ui_scene_base.get_screen_node();
		let screen_width = screen.width_anchor();
		let safe_area = self.ui_scene_base.get_safe_area_layout_guide();
		let (safe_left, safe_top, safe_right, safe_bottom) = (safe_area.left_anchor(), safe_area.top_anchor(), safe_area.right_anchor(), safe_area.bottom_anchor());
		let node_tree = self.ui_scene_base.get_scene_base_mut().get_node_tree_mut();
		let root_id = node_tree.get_root_id();

		// 머리 띠.
		let header_id = LayoutScene::create_ui_node(node_tree, root_id, "header", "#3a5a8c");
		LayoutScene::constrain(node_tree, header_id, |header| vec![header.left_anchor().equal_to(safe_left.clone()), header.top_anchor().equal_to(safe_top.clone()), header.right_anchor().equal_to(safe_right.clone()), header.height_anchor().equal_to(72)]);
		let (header_center_x, header_center_y, header_bottom) = {
			let header = LayoutScene::read_ui_node(node_tree, header_id);
			(header.center_x_anchor(), header.center_y_anchor(), header.bottom_anchor())
		};

		// 제목. (크기는 글의 자연 크기)
		let title_id = LayoutScene::create_ui_node(node_tree, header_id, "title", "");
		if let Some(title_label) = node_tree.add_component(title_id, UILabel::create_with_text("오토레이아웃 (UIScene + UINode)", 30.0)) {
			title_label.set_text_color("#ffffff");
		}
		LayoutScene::constrain(node_tree, title_id, |title| vec![title.center_x_anchor().equal_to(header_center_x.clone()), title.center_y_anchor().equal_to(header_center_y.clone())]);
		node_tree[title_id].add_component(Paint::create_with_color(Color::create(1.0, 1.0, 1.0, 0.12)));

		// 옆 판. (화면 25% 를 바라되 200 ~ 320)
		let side_id = LayoutScene::create_ui_node(node_tree, root_id, "side", "#2a3346");
		let preferred_strength = LayoutStrength::from_priority(LayoutPriority::DEFAULT_LOW);
		LayoutScene::constrain(node_tree, side_id, |side| vec![
			side.left_anchor().equal_to(safe_left.add(12)),
			side.top_anchor().equal_to(header_bottom.add(12)),
			side.bottom_anchor().equal_to(safe_bottom.subtract(12)),
			side.width_anchor().equal_to(screen_width.multiply(0.25)).with_strength(preferred_strength),
			side.width_anchor().greater_than_or_equal_to(200),
			side.width_anchor().less_than_or_equal_to(320),
		]);
		let side_right = LayoutScene::read_ui_node(node_tree, side_id).right_anchor();

		// 본문. (여백 가이드 24)
		let body_id = LayoutScene::create_ui_node(node_tree, root_id, "body", "#1f2636");
		LayoutScene::constrain(node_tree, body_id, |body| vec![body.left_anchor().equal_to(side_right.add(12)), body.top_anchor().equal_to(header_bottom.add(12)), body.right_anchor().equal_to(safe_right.subtract(12)), body.bottom_anchor().equal_to(safe_bottom.subtract(12))]);
		let (margin_left, margin_top, margin_right) = node_tree.with_component_mut::<UINode, _>(body_id, |body, _tree| {
			body.set_layout_margins(LayoutMargins::create(24.0, 24.0, 24.0, 24.0)).expect("margins");
			let guide = body.get_layout_margins_guide();
			(guide.left_anchor(), guide.top_anchor(), guide.right_anchor())
		}).expect("body");

		// 같은 너비 단추 셋. (사이 16)
		let button_colors = ["#e07a5f", "#81b29a", "#f2cc8f"];
		let mut button_ids: Vec<NodeId> = Vec::new();
		for (button_index, button_color) in button_colors.iter().enumerate() {
			let button_id = LayoutScene::create_ui_node(node_tree, body_id, &format!("button{}", button_index), button_color);
			if let Some(button_label) = node_tree.add_component(button_id, UILabel::create_with_text(&format!("단추 {}", button_index + 1), 24.0)) {
				button_label.set_text_color("#1d1d1d");
			}
			button_ids.push(button_id);
		}
		for (button_index, button_id) in button_ids.iter().copied().enumerate() {
			let previous = if button_index > 0 {
				let previous_node = LayoutScene::read_ui_node(node_tree, button_ids[button_index - 1]);
				Some((previous_node.right_anchor(), previous_node.width_anchor()))
			}
			else {
				None
			};
			let is_last = button_index + 1 == button_ids.len();
			let (margin_left, margin_top, margin_right) = (margin_left.clone(), margin_top.clone(), margin_right.clone());
			// 단추 글의 자연 너비로 줄어들려는 힘(hug)을 낮춰 옆 판의 바라는 너비를 밀어내지 않게 한다.
			node_tree.with_component_mut::<UINode, _>(button_id, |button, _tree| {
				button.set_content_hugging_priority(LayoutPriority::FITTING_SIZE_LEVEL, LayoutConstraintAxis::Horizontal).expect("hugging");
			});
			LayoutScene::constrain(node_tree, button_id, move |button| {
				let mut constraints = vec![button.top_anchor().equal_to(margin_top), button.height_anchor().equal_to(64)];
				match previous {
					Some((previous_right, previous_width)) => {
						constraints.push(button.left_anchor().equal_to(previous_right.add(16)));
						constraints.push(button.width_anchor().equal_to(previous_width));
					}
					None => {
						constraints.push(button.left_anchor().equal_to(margin_left));
					}
				}
				if is_last {
					constraints.push(button.right_anchor().equal_to(margin_right));
				}
				constraints
			});
		}

		// 자연 크기 글. (왼쪽 위만 정하고 크기는 글이 정한다)
		let note_id = LayoutScene::create_ui_node(node_tree, body_id, "note", "");
		node_tree[note_id].add_component(Paint::create_with_color(Color::create(1.0, 1.0, 1.0, 0.08)));
		if let Some(note_label) = node_tree.add_component(note_id, UILabel::create_with_text("이 글 칸의 크기는 글의 자연 크기다", 22.0)) {
			note_label.set_text_color("#d0d8e8");
		}
		let first_button_bottom = LayoutScene::read_ui_node(node_tree, button_ids[0]).bottom_anchor();
		LayoutScene::constrain(node_tree, note_id, |note| vec![note.left_anchor().equal_to(margin_left.clone()), note.top_anchor().equal_to(first_button_bottom.add(24))]);

		self.title_id = Some(title_id);
		if std::env::var("VANILLA_EXAMPLE_DEVTOOLS").map(|value| value == "1").unwrap_or(false) {
			if let Some(dev_tools) = self.ui_scene_base.get_game_scene_base_mut().get_dev_tools_mut() {
				dev_tools.set_visible(engine, true);
				dev_tools.set_selected_node(Some(side_id));
			}
		}
	}

	//==============================================================================
	// 갱신. (1 초 뒤 제목 글을 바꿔 자연 크기가 따라가는지 보인다)
	//==============================================================================
	fn tick(&mut self, _engine: &Engine, time_delta: f64) {
		self.elapsed_seconds += time_delta;
		if self.is_title_changed || self.elapsed_seconds < 1.0 {
			return;
		}
		self.is_title_changed = true;
		if let Some(title_id) = self.title_id {
			if let Some(title_label) = self.ui_scene_base.get_scene_base_mut().get_node_tree_mut()[title_id].get_component_mut::<UILabel>() {
				title_label.set_text("제목 글이 길어지면 칸도 자연 크기로 따라 커진다");
			}
		}
	}
}

vanilla::ui_scene!(LayoutScene);

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("UI 씬과 오토레이아웃 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let mut ui_scene_base = UISceneBase::new();
	ui_scene_base.set_loading_min_duration_ms(0.0);
	ui_scene_base.set_scene_background_color("#10141c");
	let layout_scene = LayoutScene {
		ui_scene_base,
		title_id: None,
		elapsed_seconds: 0.0,
		is_title_changed: false,
	};
	if let Err(error) = engine.run(Box::new(layout_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
