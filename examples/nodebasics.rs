//==============================================================================
// 노드와 컴포넌트 예제. (README 의 "노드와 컴포넌트" 코드를 사각형 셋과 제목으로 늘린 것)
// - 씬의 노드 트리에 노드를 만들고 Paint, Text 와 직접 만든 Spin 컴포넌트를 붙인다.
// - 실행: cargo run --example nodebasics
//==============================================================================

use vanilla::{Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, Paint, Pivot, Scene, SceneBase, Text, TextAlign, Vector2};

//==============================================================================
// 노드를 돌리는 컴포넌트.
//==============================================================================
struct Spin {
	component_base: ComponentBase,
	degrees_per_second: f64,
}

#[vanilla::reflect]
impl Spin {}

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
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 초기화. (노드 트리를 만든다)
	//==============================================================================
	fn initialize(&mut self, _engine: &Engine) {
		self.scene_base.set_scene_background_color("#1e1915");
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();

		// 도는 사각형 셋.
		for square_index in 0..3 {
			let square_id = node_tree.create_child(root_id, "square");
			let square_node = &mut node_tree[square_id];
			square_node.set_local_position(Vector2::create(340.0 + square_index as f64 * 300.0, 380.0));
			square_node.set_content_size(Vector2::create(160.0, 160.0));
			square_node.set_pivot(Pivot::MIDDLE_CENTER);
			square_node.add_component(Paint::create_with_color("#f3e5ab"));
			square_node.add_component(Spin {
				component_base: ComponentBase::new(),
				degrees_per_second: 45.0 * (square_index + 1) as f64,
			});
		}

		// 제목 글자.
		let title_id = node_tree.create_child(root_id, "title");
		let title_node = &mut node_tree[title_id];
		title_node.set_local_position(Vector2::create(64.0, 64.0));
		title_node.set_pivot(Pivot::TOP_LEFT);
		title_node.set_content_size(Vector2::create(800.0, 60.0));
		let title_text = title_node.add_component(Text::create_with_text("바닐라엔진", 48.0));
		title_text.set_text_color("#f3e5ab");
		title_text.set_text_align(TextAlign::Left);
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("노드와 컴포넌트 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
