//==============================================================================
// 개발자 도구 예제. (DevTools, 원본 F2 하이어라키/인스펙터 패널)
// - 씬이 DevTools 를 품고 tick 에서 tick, post_draw 에서 draw 를 부른다. 시작하면 패널을 연다. (F2 로 닫고 연다)
// - 노드 몇 개(Paint, Text, 직접 만든 Spin 컴포넌트, 비활성 노드, 중첩)를 놓고 "card" 를 골라 인스펙터에 보인다.
// - Spin 은 #[vanilla::reflect] 를 붙여, 인스펙터가 엔진 컴포넌트처럼 게터(degreesPerSecond)를 불러 보인다.
// - 환경 변수 VANILLA_DEVTOOLS_TAB 에 0 ~ 3 을 주면 그 탭으로 연다. (0 Statistics, 1 Node Hierarchy, 2 Local Storage, 3 Settings)
// - 실행: cargo run --example devtoolsshowcase
//==============================================================================

use vanilla::{Component, ComponentBase, ComponentContext, DevTools, Engine, EngineConfiguration, Graphic, LocalStorage, NodeId, Paint, Pivot, Scene, SceneBase, Text, TextAlign, Vector2};

//==============================================================================
// 노드를 돌리는 컴포넌트. (반사 매크로를 붙여 인스펙터에 속성이 보인다)
//==============================================================================
struct Spin {
	component_base: ComponentBase,
	degrees_per_second: f64,
}

#[vanilla::reflect]
impl Spin {
	//==============================================================================
	// 초당 도는 각도 설정.
	//==============================================================================
	pub fn set_degrees_per_second(&mut self, degrees_per_second: f64) {
		self.degrees_per_second = degrees_per_second;
	}

	//==============================================================================
	// 초당 도는 각도 반환.
	//==============================================================================
	pub fn get_degrees_per_second(&self) -> f64 {
		self.degrees_per_second
	}
}

#[vanilla::reflect]
impl Component for Spin {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Spin"
	}

	//==============================================================================
	// 주기적 갱신. (자기 노드를 돌린다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
		let transform = context.get_transform_mut();
		let local_rotation = transform.get_local_rotation();
		transform.set_local_rotation(local_rotation + self.degrees_per_second * time_delta);
	}
}

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	scene_base: SceneBase,
	dev_tools: DevTools,
	// 처음 tick 에서 패널을 열었는지.
	is_dev_tools_opened: bool,
	// 처음 고를 노드.
	selected_node_id: Option<NodeId>,
}

impl MainScene {
	//==============================================================================
	// 글 노드 만들기.
	//==============================================================================
	fn create_label(&mut self, parent_id: NodeId, name: &str, text: &str, position: Vector2) -> NodeId {
		let node_tree = self.scene_base.get_node_tree_mut();
		let label_id = node_tree.create_child(parent_id, name);
		let label_node = &mut node_tree[label_id];
		label_node.set_local_position(position);
		label_node.set_pivot(Pivot::TOP_LEFT);
		label_node.set_content_size(Vector2::create(360.0, 32.0));
		let label_text = label_node.add_component(Text::create_with_text(text, 24.0));
		label_text.set_text_color("#f3e5ab");
		label_text.set_text_align(TextAlign::Left);
		label_id
	}
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 초기화. (노드 트리를 만들고 DevTools 에 루트를 준다)
	//==============================================================================
	fn initialize(&mut self, _engine: &Engine) {
		self.scene_base.set_scene_background_color("#1e1915");
		let root_id = self.scene_base.get_root_id();
		self.create_label(root_id, "title", "개발자 도구 (F2)", Vector2::create(40.0, 30.0));

		// 카드: 바탕 Paint 와 그 안의 글, 도는 사각형.
		let node_tree = self.scene_base.get_node_tree_mut();
		let card_id = node_tree.create_child(root_id, "card");
		let card_node = &mut node_tree[card_id];
		card_node.set_local_position(Vector2::create(40.0, 90.0));
		card_node.set_pivot(Pivot::TOP_LEFT);
		card_node.set_content_size(Vector2::create(380.0, 220.0));
		let card_paint = card_node.add_component(Paint::create_with_color("#3a2f22"));
		card_paint.set_round_size(12.0);
		self.create_label(card_id, "caption", "카드 안의 글", Vector2::create(20.0, 20.0));
		let node_tree = self.scene_base.get_node_tree_mut();
		let spinner_id = node_tree.create_child(card_id, "spinner");
		let spinner_node = &mut node_tree[spinner_id];
		spinner_node.set_local_position(Vector2::create(300.0, 150.0));
		spinner_node.set_content_size(Vector2::create(60.0, 60.0));
		spinner_node.add_component(Paint::create_with_color("#d4b46a"));
		spinner_node.add_component(Spin {
			component_base: ComponentBase::new(),
			degrees_per_second: 60.0,
		});
		let badge_id = node_tree.create_child(spinner_id, "badge");
		let badge_node = &mut node_tree[badge_id];
		badge_node.set_local_position(Vector2::create(30.0, 30.0));
		badge_node.set_content_size(Vector2::create(16.0, 16.0));
		badge_node.add_component(Paint::create_with_color("#a8c870"));

		// 비활성 노드.
		let hidden_id = node_tree.create_child(root_id, "hidden");
		let hidden_node = &mut node_tree[hidden_id];
		hidden_node.set_local_position(Vector2::create(40.0, 340.0));
		hidden_node.set_pivot(Pivot::TOP_LEFT);
		hidden_node.set_content_size(Vector2::create(200.0, 60.0));
		hidden_node.add_component(Paint::create_with_color("#c87878"));
		hidden_node.set_active(false);
		self.create_label(hidden_id, "hiddenLabel", "숨은 노드", Vector2::create(10.0, 10.0));

		// 빈 묶음 노드.
		let node_tree = self.scene_base.get_node_tree_mut();
		let group_id = node_tree.create_child(root_id, "group");
		for item_index in 0..3 {
			let item_id = node_tree.create_child(group_id, &format!("item{}", item_index));
			let item_node = &mut node_tree[item_id];
			item_node.set_local_position(Vector2::create(60.0 + item_index as f64 * 50.0, 450.0));
			item_node.set_content_size(Vector2::create(36.0, 36.0));
			item_node.add_component(Paint::create_with_color("#7a6a50"));
		}

		// 로컬 스토리지 탭에 보일 값.
		LocalStorage::set_string("showcase.playerName", "바닐라");
		LocalStorage::set_number("showcase.bestScore", 1234.0);

		self.dev_tools.set_root_nodes(vec![root_id]);
		self.dev_tools.set_node_expanded(card_id, true);
		self.dev_tools.set_node_expanded(spinner_id, true);
		self.selected_node_id = Some(card_id);
	}

	//==============================================================================
	// 주기적 갱신. (노드 다음에 DevTools, 처음 한 번 패널을 연다)
	//==============================================================================
	fn tick(&mut self, engine: &Engine, time_delta: f64) {
		self.scene_base.tick(engine, time_delta);
		if !self.is_dev_tools_opened {
			self.is_dev_tools_opened = true;
			self.dev_tools.set_visible(engine, true);
			self.dev_tools.set_selected_node(self.selected_node_id);
			let tab_index = std::env::var("VANILLA_DEVTOOLS_TAB").ok().and_then(|tab_text| tab_text.parse::<usize>().ok()).unwrap_or(1);
			self.dev_tools.set_active_tab(tab_index);
		}
		let unscaled_time_delta = engine.get_time_manager().get_unscale_delta_time();
		let node_tree = self.scene_base.get_node_tree_mut();
		self.dev_tools.tick(engine, node_tree, unscaled_time_delta);
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (DevTools 패널)
	//==============================================================================
	fn post_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.dev_tools.draw(engine, self.scene_base.get_node_tree(), graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("개발자 도구 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
		dev_tools: DevTools::new(),
		is_dev_tools_opened: false,
		selected_node_id: None,
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
