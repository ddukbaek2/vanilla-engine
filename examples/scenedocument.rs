//==============================================================================
// UI 문서 예제. (UIDocument 로 씬 파일을 읽어 노드 트리를 짓고 그린다)
// - examples/assets/scenedocument.scene.json 은 vanilla.js 의 UIDocument 와 비주얼 에디터 저장 꼴(문서에 frameSize 를 얹은 것)로 만든 씬이다.
//   Panel 아래 제목 글, 그림(Sprite, 돌린 Sprite, UIImageView), 테(Outline), 진행 고리, 라벨, 단추(content 안의 글), 슬라이더, 진행 막대, 스크롤 뷰(content 안의 줄 넷), 별 셋이 있다.
// - 그림 경로는 애셋 폴더 후보에서 찾는다. (이 저장소의 examples/assets, 원본 저장소의 timelineshowcase 그림 폴더)
// - 씬 파일은 create 에서 TextAsset 으로 요청해 맡기고, initialize 에서 그림을 요청한 뒤(load_images) 트리를 짓는다.
//   웹처럼 그림이 나중에 읽히면 tick 의 bind_pending_images 가 붙인다.
// - 지은 트리를 to_json_text 로 다시 내보내 읽은 문서와 같은지 화면 아래에 적는다.
// - 실행: cargo run --example scenedocument
//   화면 저장: VANILLA_CAPTURE_PATH=shots/scenedocument.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use serde_json::Value;
use std::path::PathBuf;
use vanilla::{Color, Engine, EngineConfiguration, Graphic, NodeId, NodeTree, Pivot, Scene, SceneBase, Text, TextAlign, TextAsset, UIDocument, Vector2};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const SCENE_FILE_PATH: &str = "scenedocument.scene.json";
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets";
const IMAGE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/timelineshowcase/assets";
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;

//==============================================================================
// UI 문서 씬.
//==============================================================================
struct SceneDocumentScene {
	scene_base: SceneBase,
	scene_asset: TextAsset,
	document: UIDocument,
	pending_image_count: usize,
	status_label_id: Option<NodeId>,
	status_text: String,
}

impl SceneDocumentScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> SceneDocumentScene {
		SceneDocumentScene {
			scene_base: SceneBase::new(),
			scene_asset: TextAsset::new(),
			document: UIDocument::new(),
			pending_image_count: 0,
			status_label_id: None,
			status_text: String::new(),
		}
	}

	//==============================================================================
	// 노드 수 세기. (뿌리와 그 자손)
	//==============================================================================
	fn count_nodes(node_tree: &NodeTree, node_id: NodeId) -> usize {
		let mut node_count = 1;
		for child_id in node_tree.get_child_ids(node_id).iter() {
			node_count += SceneDocumentScene::count_nodes(node_tree, *child_id);
		}
		node_count
	}

	//==============================================================================
	// 상태 줄 갱신. (아직 붙지 않은 그림 수를 함께 적는다)
	//==============================================================================
	fn refresh_status_label(&mut self) {
		let status_line = if self.pending_image_count > 0 {
			format!("{}  읽는 중인 그림 {}개", self.status_text, self.pending_image_count)
		}
		else {
			self.status_text.clone()
		};
		if let Some(status_label_id) = self.status_label_id {
			if let Some(text) = self.scene_base.get_node_tree_mut()[status_label_id].get_component_mut::<Text>() {
				text.set_text(&status_line);
			}
		}
	}
}

impl Scene for SceneDocumentScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (씬 파일을 요청해 맡긴다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.scene_asset.load(engine, SCENE_FILE_PATH);
		self.scene_base.track_asset(&self.scene_asset);
	}

	//==============================================================================
	// 초기화. (그림을 요청하고 문서로 트리를 지은 뒤, 다시 내보내 견준다)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color("#0d1117");
		let scene_text = match self.scene_asset.get_text() {
			Some(scene_text) => {
				scene_text.to_string()
			}
			None => {
				eprintln!("scenedocument: 씬 파일을 읽지 못했습니다 ({})", SCENE_FILE_PATH);
				return;
			}
		};
		self.document.load_images(engine, &scene_text);

		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let document_root_id = match self.document.from_json_text(node_tree, &scene_text) {
			Ok(document_root_id) => {
				document_root_id
			}
			Err(error) => {
				eprintln!("scenedocument: {}", error);
				return;
			}
		};
		node_tree.add_child(root_id, document_root_id);

		// 다시 내보내 읽은 문서의 root 와 견준다. (에디터 값 frameSize 는 문서 밖이라 빼고 본다)
		let exported_text = self.document.to_json_text(node_tree, document_root_id, true);
		let read_root = serde_json::from_str::<Value>(&scene_text).ok().map(|document_data| document_data["root"].clone());
		let exported_root = serde_json::from_str::<Value>(&exported_text).ok().map(|document_data| document_data["root"].clone());
		let comparison = if read_root.is_some() && read_root == exported_root {
			"같다"
		}
		else {
			"다르다"
		};
		let node_count = SceneDocumentScene::count_nodes(node_tree, document_root_id);
		self.status_text = format!("씬 파일 {} 에서 노드 {}개를 지었다. 다시 내보낸 문서({}자)가 읽은 문서와 {}.", SCENE_FILE_PATH, node_count, exported_text.chars().count(), comparison);

		// 상태 줄. (문서 밖에서 코드로 만든 노드)
		let status_label_id = node_tree.create_child(root_id, "status");
		let status_label = &mut node_tree[status_label_id];
		status_label.set_pivot(Pivot::TOP_LEFT);
		status_label.set_local_position(Vector2::create(60.0, 690.0));
		status_label.set_content_size(Vector2::create(1160.0, 24.0));
		let status_text = status_label.add_component(Text::create_with_text("", 18.0));
		status_text.set_text_align(TextAlign::Left);
		status_text.set_text_color(Color::create(0.75, 0.8, 0.9, 1.0));
		self.status_label_id = Some(status_label_id);
		self.pending_image_count = self.document.bind_pending_images(self.scene_base.get_node_tree_mut());
		self.refresh_status_label();
	}

	//==============================================================================
	// 갱신. (나중에 읽힌 그림을 붙인다)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		if self.pending_image_count > 0 {
			self.pending_image_count = self.document.bind_pending_images(self.scene_base.get_node_tree_mut());
			self.refresh_status_label();
		}
		self.scene_base.tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("UI 문서 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY), PathBuf::from(IMAGE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(SceneDocumentScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
