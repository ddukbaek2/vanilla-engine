//==============================================================================
// UI 애셋 샘플 예제. (원본 examples/uisample 과 같은 장면)
// - UIEditor 로 만든 uiasset(JSON) 을 UIDocument 로 그대로 불러와 화면을 짓는다. 불러온 노드는 살아 있는 엔진 UI 노드이므로 이름으로 찾아 동작을 붙인다.
//   시작, 설정 단추를 누르면 아래 상태 줄이 바뀌고, 진행 막대는 천천히 오르내리며, 목록은 위아래로 끌 수 있다.
// - 상태 줄은 문서에 없는 노드를 코드로 덧붙인 것이다. (문서 밖 요소도 함께 쓸 수 있다는 보기)
// - examples/assets/uisample/title.uiasset.json 은 원본 저장소 examples/uisample/assets 와 같은 파일이다.
// - 실행: cargo run --release --example uisample
//   화면 저장: VANILLA_CAPTURE_PATH=shots/uisample.png VANILLA_CAPTURE_FRAMES=60 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::{Color, ComponentContext, Engine, EngineConfiguration, Graphic, NodeId, Pivot, Scene, SceneBase, TextAsset, TouchRecognizer, UIButton, UIDocument, UILabel, UIProgressView, Vector2};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const DOCUMENT_FILE_PATH: &str = "title.uiasset.json";
const ASSET_DIRECTORY: &str = "examples/assets/uisample";
const VIEW_WIDTH: f64 = 960.0;
const VIEW_HEIGHT: f64 = 640.0;

//==============================================================================
// UI 애셋 샘플 씬.
//==============================================================================
struct UISampleScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	document_asset: TextAsset,
	document: UIDocument,
	progress_node_id: Option<NodeId>,
	status_node_id: Option<NodeId>,
	status_text: Rc<RefCell<Option<String>>>,
	elapsed_seconds: f64,
}

impl UISampleScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> UISampleScene {
		UISampleScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			document_asset: TextAsset::new(),
			document: UIDocument::new(),
			progress_node_id: None,
			status_node_id: None,
			status_text: Rc::new(RefCell::new(None)),
			elapsed_seconds: 0.0,
		}
	}

	//==============================================================================
	// 이름으로 찾은 단추에 눌림 동작 붙이기.
	//==============================================================================
	fn attach_click(&mut self, document_root_id: NodeId, node_name: &str, message: &'static str) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let button_node_id = match node_tree.find_descendant_by_name(document_root_id, node_name) {
			Some(button_node_id) => button_node_id,
			None => return,
		};
		let status_text = self.status_text.clone();
		if let Some(button) = node_tree[button_node_id].get_component_mut::<UIButton>() {
			button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
				*status_text.borrow_mut() = Some(String::from(message));
			}));
		}
	}
}

impl Scene for UISampleScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (uiasset 파일을 요청해 맡긴다)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		self.document_asset.load(engine, DOCUMENT_FILE_PATH);
		self.scene_base.track_asset(&self.document_asset);
	}

	//==============================================================================
	// 초기화. (문서를 노드 트리로 복원하고 이름으로 찾아 동작을 붙인다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let _ = engine;
		self.scene_base.set_scene_background_color("rgb(16, 19, 26)");
		let document_text = match self.document_asset.get_text() {
			Some(document_text) => document_text.to_string(),
			None => {
				eprintln!("uisample: uiasset 파일을 읽지 못했습니다 ({})", DOCUMENT_FILE_PATH);
				return;
			}
		};
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let document_root_id = match self.document.from_json_text(node_tree, &document_text) {
			Ok(document_root_id) => document_root_id,
			Err(error) => {
				eprintln!("uisample: {}", error);
				return;
			}
		};
		node_tree.add_child(root_id, document_root_id);
		self.attach_click(document_root_id, "StartButton", "시작을 눌렀습니다.");
		self.attach_click(document_root_id, "OptionButton", "설정을 눌렀습니다.");

		let node_tree = self.scene_base.get_node_tree_mut();
		self.progress_node_id = node_tree.find_descendant_by_name(document_root_id, "ProgressTrack");

		// 상태 표시줄은 코드로 덧붙인다. (문서에 없는 요소도 함께 쓸 수 있다는 예시)
		let status_node_id = node_tree.create_child(root_id, "Status");
		let status_node = &mut node_tree[status_node_id];
		status_node.set_pivot(Pivot::TOP_LEFT);
		status_node.set_content_size(Vector2::create(520.0, 26.0));
		status_node.set_local_position(Vector2::create(220.0, 560.0));
		let status_label = status_node.add_component(UILabel::create_with_text("버튼을 눌러 보세요. 목록은 위아래로 끌 수 있습니다.", 15.0));
		status_label.set_text_color(Color::create(0.55, 0.6, 0.68, 1.0));
		self.status_node_id = Some(status_node_id);
	}

	//==============================================================================
	// 갱신. (눌림 문구 반영, 진행 막대를 천천히 채워 살아 있는 화면임을 보인다)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		self.elapsed_seconds += time_delta;
		let pending_text = self.status_text.borrow_mut().take();
		let node_tree = self.scene_base.get_node_tree_mut();
		if let (Some(text), Some(status_node_id)) = (pending_text, self.status_node_id) {
			if let Some(status_label) = node_tree[status_node_id].get_component_mut::<UILabel>() {
				status_label.set_text(&text);
			}
		}
		if let Some(progress_node_id) = self.progress_node_id {
			let progress_ratio = (self.elapsed_seconds * 0.8).sin() * 0.5 + 0.5;
			if let Some(progress_view) = node_tree[progress_node_id].get_component_mut::<UIProgressView>() {
				progress_view.set_value(progress_ratio);
			}
		}
		node_tree.tick(engine, time_delta);
		self.touch_recognizer.tick_touch(self.scene_base.get_node_tree_mut(), engine);
	}

	//==============================================================================
	// 이전 출력. (배경 칠하고 뷰 좌표계 걸기)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("UI 애셋 샘플 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(UISampleScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
