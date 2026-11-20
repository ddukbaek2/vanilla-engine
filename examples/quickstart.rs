//==============================================================================
// 빠른 시작 예제. (README 의 빠른 시작과 같은 코드)
// - 씬 하나를 만들고 흰 사각형 하나를 그린다.
// - 실행: cargo run --example quickstart
//==============================================================================

use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

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
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.set_fill_color(Colors::WHITE);
		graphic.draw_rect(Rect::create(100.0, 100.0, 200.0, 120.0));
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진");
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
