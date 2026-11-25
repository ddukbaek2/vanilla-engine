//==============================================================================
// async/await 예제. (원본 vanilla.js 의 async load, await asset.load, Wait.seconds, fadeScreen(async () => ...) 를 같은 꼴로 쓴다)
// - 불러오기: load_assets 에서 불러오기 작업을 맡기고 씬에 맡긴다. 작업은 원본 async 함수처럼 맡기는 자리에서 첫 await 까지 돌고,
//   씬의 아틀라스가 load 가 돌려준 손잡이로 JSON 과 쪽 그림을 await 로 기다린 뒤 0.5초 쉰다.
//   작업이 끝날 때까지 GameSceneBase 의 로딩 화면이 보인다. (원본 async load 를 기다리는 동안의 drawOnLoad)
// - 이야기: initialize 에서 맡긴 씬 작업 하나가 원본 async 함수처럼 위에서 아래로 돈다. 글을 바꾸고, 1초 기다리고(wait::seconds),
//   아이콘을 한 프레임씩 옮기고(wait::next_frame), 화면이 어두운 동안 아이콘을 바꾼다(fade_screen_async). 페이드가 끝나기는 Promise 로 기다린다.
//   await 다음 줄은 scene_context.with(|씬, 엔진| ...) 안에서 씬과 엔진을 만진다. (원본 async 함수 안의 this 자리)
// - 원본 JS 와 나란히:
//     async loadAssets() {                                   fn load_assets(&mut self, engine: &mut Engine) {
//         await super.loadAssets();                              let load_task_handle = engine.spawn_scene_task(self, |mut scene_context| async move {
//         await this.atlas.load("atlas/icons.json");                 scene_context.with(|scene, engine| scene.sprite_atlas.load(engine, "atlas/icons.json")).await;
//         await Wait.seconds(0.5);                                   wait::seconds(0.5).await;
//     }                                                          });
//                                                                self.game_scene_base.get_scene_base_mut().track_task(&load_task_handle);
//                                                            }
// - 실행: cargo run --example asyncawait
//   화면 저장: VANILLA_CAPTURE_PATH=shots/async_{frame}.png VANILLA_CAPTURE_FRAMES=20,100,150,205 VANILLA_CAPTURE_EXIT=1
//==============================================================================
use std::path::PathBuf;
use vanilla::{wait, Engine, EngineConfiguration, GameScene, GameSceneBase, NodeId, NodeTree, Pivot, Promise, Sprite, SpriteAtlas, Text, TextAlign, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets";
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#151821";
const TEXT_COLOR: &str = "#e8ebf2";
const CAPTION_COLOR: &str = "#9aa2b2";
const ACCENT_COLOR: &str = "#ffcc33";
const ICON_FRAME_NAMES: [&str; 5] = ["star", "heart", "leaf", "gem", "coin"];
const ICON_SIZE: f32 = 96.0;
const ICON_START_X: f32 = 240.0;
const ICON_END_X: f32 = 1040.0;
const ICON_Y: f32 = 300.0;
const ICON_MOVE_FRAME_COUNT: u32 = 60;
// 원본 JS 와 러스트를 나란히 보이는 줄. (원본, 러스트)
const CODE_PAIR_LIST: [(&str, &str); 4] = [
	("await this.atlas.load(\"atlas/icons.json\")", "context.with(|scene, engine| scene.atlas.load(engine, ...)).await"),
	("await Wait.seconds(1)", "wait::seconds(1.0).await"),
	("await Wait.nextFrame()", "wait::next_frame().await"),
	("this.fadeScreen(async () => { ... })", "scene.fade_screen_async(|context| async move { ... })"),
];

//==============================================================================
// 글 노드 만들기. (왼쪽 위 피벗)
//==============================================================================
fn create_text_node(node_tree: &mut NodeTree, parent_id: NodeId, x: f32, y: f32, width: f32, text: &str, font_size: f32, color: &str) -> NodeId {
	let text_id = node_tree.create_child(parent_id, "text");
	let text_node = &mut node_tree[text_id];
	text_node.set_pivot(Pivot::TOP_LEFT);
	text_node.set_rect(x, y, width, font_size * 1.5);
	let text_component = text_node.add_component(Text::create_with_text(text, font_size));
	text_component.set_text_color(color);
	text_component.set_text_align(TextAlign::Left);
	text_id
}

//==============================================================================
// async/await 씬.
//==============================================================================
struct AsyncAwaitScene {
	game_scene_base: GameSceneBase,
	sprite_atlas: SpriteAtlas,
	status_node_id: Option<NodeId>,
	icon_node_id: Option<NodeId>,
	icon_frame_index: usize,
}

impl AsyncAwaitScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> AsyncAwaitScene {
		let mut game_scene_base = GameSceneBase::new();
		game_scene_base.set_view_scale_mode(ViewScaleMode::StretchShort, None);
		game_scene_base.set_loading_min_duration_ms(0.0);
		game_scene_base.set_scene_background_color(BACKGROUND_COLOR);
		AsyncAwaitScene {
			game_scene_base,
			sprite_atlas: SpriteAtlas::new(),
			status_node_id: None,
			icon_node_id: None,
			icon_frame_index: 0,
		}
	}

	//==============================================================================
	// 지금 하는 일 글 바꾸기.
	//==============================================================================
	fn set_status(&mut self, status: &str) {
		let status_node_id = match self.status_node_id {
			Some(status_node_id) => {
				status_node_id
			}
			None => {
				return;
			}
		};
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		if let Some(status_text) = node_tree[status_node_id].get_component_mut::<Text>() {
			status_text.set_text(status);
		}
	}

	//==============================================================================
	// 아이콘 옮기기. (0 이면 왼쪽 끝, 1 이면 오른쪽 끝)
	//==============================================================================
	fn move_icon(&mut self, progress: f32) {
		let icon_node_id = match self.icon_node_id {
			Some(icon_node_id) => {
				icon_node_id
			}
			None => {
				return;
			}
		};
		let icon_x = ICON_START_X + (ICON_END_X - ICON_START_X) * progress;
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		node_tree[icon_node_id].set_rect(icon_x, ICON_Y, ICON_SIZE, ICON_SIZE);
	}

	//==============================================================================
	// 아이콘 그림을 다음 조각으로 바꾸기.
	//==============================================================================
	fn apply_next_icon_frame(&mut self) {
		let icon_node_id = match self.icon_node_id {
			Some(icon_node_id) => {
				icon_node_id
			}
			None => {
				return;
			}
		};
		self.icon_frame_index = (self.icon_frame_index + 1) % ICON_FRAME_NAMES.len();
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		if let Some(sprite) = node_tree[icon_node_id].get_component_mut::<Sprite>() {
			self.sprite_atlas.apply_frame(sprite, ICON_FRAME_NAMES[self.icon_frame_index]);
		}
	}

	//==============================================================================
	// 화면 노드 만들기. (제목, 원본과 러스트를 나란히 보이는 줄, 지금 하는 일, 아이콘)
	//==============================================================================
	fn create_nodes(&mut self) {
		let parent_id = match self.game_scene_base.get_safe_area_node_id() {
			Some(safe_area_node_id) => {
				safe_area_node_id
			}
			None => {
				self.game_scene_base.get_scene_base().get_root_id()
			}
		};
		let node_tree = self.game_scene_base.get_scene_base_mut().get_node_tree_mut();
		create_text_node(node_tree, parent_id, 60.0, 40.0, 1160.0, "async/await (원본 vanilla.js 와 같은 꼴)", 34.0, TEXT_COLOR);
		create_text_node(node_tree, parent_id, 60.0, 110.0, 560.0, "원본 JS", 18.0, CAPTION_COLOR);
		create_text_node(node_tree, parent_id, 640.0, 110.0, 600.0, "러스트", 18.0, CAPTION_COLOR);
		for (pair_index, (original_line, rust_line)) in CODE_PAIR_LIST.iter().enumerate() {
			let line_y = 140.0 + pair_index as f32 * 30.0;
			create_text_node(node_tree, parent_id, 60.0, line_y, 560.0, original_line, 18.0, TEXT_COLOR);
			create_text_node(node_tree, parent_id, 640.0, line_y, 600.0, rust_line, 18.0, ACCENT_COLOR);
		}
		let status_node_id = create_text_node(node_tree, parent_id, 60.0, 460.0, 1160.0, "", 26.0, ACCENT_COLOR);
		let icon_node_id = node_tree.create_child(parent_id, "icon");
		let icon_node = &mut node_tree[icon_node_id];
		icon_node.set_pivot(Pivot::TOP_LEFT);
		icon_node.set_rect(ICON_START_X, ICON_Y, ICON_SIZE, ICON_SIZE);
		let sprite = icon_node.add_component(Sprite::new());
		self.sprite_atlas.apply_frame(sprite, ICON_FRAME_NAMES[0]);
		self.status_node_id = Some(status_node_id);
		self.icon_node_id = Some(icon_node_id);
	}
}

impl GameScene for AsyncAwaitScene {
	fn get_game_scene_base(&self) -> &GameSceneBase {
		&self.game_scene_base
	}

	fn get_game_scene_base_mut(&mut self) -> &mut GameSceneBase {
		&mut self.game_scene_base
	}

	//==============================================================================
	// 불러오기. (원본 async loadAssets: 아틀라스를 await 로 받고 0.5초 쉰다. 맡긴 작업이 끝나야 initialize 가 불린다)
	//==============================================================================
	fn load_assets(&mut self, engine: &mut Engine) {
		let load_task_handle = engine.spawn_scene_task(self, |mut scene_context| async move {
			scene_context.with(|scene, engine| scene.sprite_atlas.load(engine, "atlas/icons.json")).await;
			wait::seconds(0.5).await;
		});
		self.game_scene_base.get_scene_base_mut().track_task(&load_task_handle);
	}

	//==============================================================================
	// 초기화. (노드를 만들고, 원본 async 함수처럼 차례로 도는 이야기 작업을 맡긴다)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.create_nodes();
		engine.spawn_scene_task(self, |mut scene_context| async move {
			let mut round = 0;
			loop {
				round += 1;
				scene_context.with(|scene, _engine| scene.set_status(&format!("{} 번째: 1초 기다림 (wait::seconds)", round)));
				wait::seconds(1.0).await;

				scene_context.with(|scene, _engine| scene.set_status("아이콘을 한 프레임씩 옮김 (wait::next_frame)"));
				for move_frame in 0..ICON_MOVE_FRAME_COUNT {
					let progress = move_frame as f32 / (ICON_MOVE_FRAME_COUNT - 1) as f32;
					scene_context.with(|scene, _engine| scene.move_icon(progress));
					wait::next_frame().await;
				}

				// 어두운 동안 아이콘을 바꾸고 0.4초 더 어둡게 둔다. 페이드 작업이 끝나기는 Promise 로 기다린다.
				scene_context.with(|scene, _engine| scene.set_status("어두운 동안 아이콘을 바꿈 (fade_screen_async)"));
				let (fade_promise, fade_resolver) = Promise::create();
				scene_context.with(move |scene, _engine| {
					scene.fade_screen_async(move |mut fade_context| async move {
						fade_context.with(|fade_scene, _engine| {
							fade_scene.move_icon(0.0);
							fade_scene.apply_next_icon_frame();
						});
						wait::seconds(0.4).await;
						fade_resolver.resolve(());
					});
				});
				fade_promise.await;
			}
		});
	}
}

vanilla::game_scene!(AsyncAwaitScene);

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("async/await (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(AsyncAwaitScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
