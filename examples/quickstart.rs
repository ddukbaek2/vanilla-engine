//==============================================================================
// 빠른 시작 예제. (README 의 빠른 시작과 같은 코드)
// - 씬 하나를 만들고 흰 사각형 하나를 그린다.
// - 실행: cargo run --example quickstart
//==============================================================================
#![allow(non_snake_case)]

use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	sceneBase: SceneBase,
}

impl Scene for MainScene {
	fn getSceneBase(&self) -> &SceneBase {
		&self.sceneBase
	}

	fn getSceneBaseMut(&mut self) -> &mut SceneBase {
		&mut self.sceneBase
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.setFillColor(Colors::white);
		graphic.drawRect(Rect::create(100.0, 100.0, 200.0, 120.0));
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("바닐라엔진");
	engineConfiguration.referenceResolutionSize = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engineConfiguration);
	let mainScene = MainScene {
		sceneBase: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(mainScene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
