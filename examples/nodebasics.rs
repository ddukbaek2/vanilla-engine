//==============================================================================
// 노드와 컴포넌트 예제. (README 의 "노드와 컴포넌트" 코드를 사각형 셋과 제목으로 늘린 것)
// - 씬의 노드 트리에 노드를 만들고 Paint, Text 와 직접 만든 Spin 컴포넌트를 붙인다.
// - 실행: cargo run --example nodebasics
//==============================================================================
#![allow(non_snake_case)]

use vanilla::{Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, Paint, Pivot, Scene, SceneBase, Text, TextAlign, Vector2};

//==============================================================================
// 노드를 돌리는 컴포넌트.
//==============================================================================
struct Spin {
	componentBase: ComponentBase,
	degreesPerSecond: f32,
}

impl Component for Spin {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"Spin"
	}

	//==============================================================================
	// 주기적 갱신. (자기 노드를 돌린다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		let transform = context.getTransformMut();
		let localRotation = transform.getLocalRotation();
		transform.setLocalRotation(localRotation + self.degreesPerSecond * timeDelta);
	}
}

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
	// 초기화. (노드 트리를 만든다)
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
		self.sceneBase.setSceneBackgroundColor("#1e1915");
		let rootId = self.sceneBase.getRootId();
		let nodeTree = self.sceneBase.getNodeTreeMut();

		// 도는 사각형 셋.
		for squareIndex in 0..3 {
			let squareId = nodeTree.createChild(rootId, "square");
			let squareNode = &mut nodeTree[squareId];
			squareNode.setLocalPosition(Vector2::create(340.0 + squareIndex as f32 * 300.0, 380.0));
			squareNode.setContentSize(Vector2::create(160.0, 160.0));
			squareNode.setPivot(Pivot::middleCenter);
			squareNode.addComponent(Paint::createWithColor("#f3e5ab"));
			squareNode.addComponent(Spin {
				componentBase: ComponentBase::new(),
				degreesPerSecond: 45.0 * (squareIndex + 1) as f32,
			});
		}

		// 제목 글자.
		let titleId = nodeTree.createChild(rootId, "title");
		let titleNode = &mut nodeTree[titleId];
		titleNode.setLocalPosition(Vector2::create(64.0, 64.0));
		titleNode.setPivot(Pivot::topLeft);
		titleNode.setContentSize(Vector2::create(800.0, 60.0));
		let titleText = titleNode.addComponent(Text::createWithText("바닐라엔진", 48.0));
		titleText.setTextColor("#f3e5ab");
		titleText.setTextAlign(TextAlign::left);
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn preDraw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.sceneBase.drawSceneBackground(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engineConfiguration = EngineConfiguration::new();
	engineConfiguration.title = String::from("노드와 컴포넌트 (바닐라엔진)");
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
