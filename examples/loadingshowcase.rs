//==============================================================================
// 로딩 쇼케이스 예제. (AssetLoader + 바꾼 GameSceneBase 로딩 화면 + FixedStepper 눈으로 확인)
// - 원본 examples/timelineshowcase 의 그림 일곱 장과 문서 하나를 AssetLoader 로 셋씩 함께 받고, 없는 파일 하나는 실패 목록에 남긴다.
//   애셋은 원본 저장소 폴더에서 읽기만 한다. (timelineshowcase 예제와 같은 방식)
// - 받은 뒤에 하는 단계(지형 짓기 같은 것)를 흉내 내어 읽기 손잡이 열 개를 add_load_handle 로 넣고 10 프레임마다 하나씩 끝낸다.
//   데스크탑은 파일이 그 자리에서 읽혀 로딩 화면이 바로 지나가므로, 이 단계가 진행률이 차오르는 모습을 보여 준다.
// - 로딩 화면은 set_loading_colors 로 색을, set_loading_drawer 로 그림(도는 점 셋, 둥근 막대, 받은 수)을 바꿨다. 막대는 LoadingProgress 가 부드럽게 따라간다.
// - 다 받으면 받은 그림, 실패 목록, FixedStepper(1/30 초 걸음)로 움직이고 남은 몫으로 섞어 그리는 공을 보인다.
// - BuildInfo 의 판도 보인다. (웹은 페이지의 meta build-version, 없으면 version.json, 둘 다 없으면 이 크레이트의 판. 읽기 손잡이도 AssetLoader 에 넣어 같이 기다린다)
// - 실행: cargo run --example loadingshowcase
//   화면 저장: VANILLA_CAPTURE_PATH=shots/loading_{frame}.png VANILLA_CAPTURE_FRAMES=45,160 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::path::PathBuf;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{AssetLoadHandle, AssetLoader, BuildInfo, Color, Engine, EngineConfiguration, FixedStepper, GameSceneBase, Graphic, ImageAsset, JsonAsset, Rect, Scene, SceneBase, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/timelineshowcase/assets";
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const IMAGE_NAME_LIST: [&str; 7] = ["emblem.png", "slime.png", "star.png", "coin.png", "bat.png", "gem.png", "cover.png"];
const STAGE_COUNT: usize = 10;
const STAGE_FRAME_INTERVAL: u64 = 10;
const ACCENT_COLOR: &str = "#ffcc33";
const STEP_SECONDS: f64 = 1.0 / 30.0;

//==============================================================================
// 로딩 쇼케이스 씬.
//==============================================================================
struct LoadingShowcaseScene {
	game_scene_base: GameSceneBase,
	asset_loader: AssetLoader,
	image_asset_list: Vec<ImageAsset>,
	json_asset: Option<JsonAsset>,
	stage_handle_list: Vec<AssetLoadHandle>,
	loading_frame_count: u64,
	fixed_stepper: FixedStepper,
	ball_position_x: f64,
	previous_ball_position_x: f64,
	ball_velocity_x: f64,
	last_alpha: f64,
	build_info: BuildInfo,
}

impl LoadingShowcaseScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> LoadingShowcaseScene {
		LoadingShowcaseScene {
			game_scene_base: GameSceneBase::new(),
			asset_loader: AssetLoader::new(),
			image_asset_list: Vec::new(),
			json_asset: None,
			stage_handle_list: Vec::new(),
			loading_frame_count: 0,
			fixed_stepper: FixedStepper::new(STEP_SECONDS, 5),
			ball_position_x: 200.0,
			previous_ball_position_x: 200.0,
			ball_velocity_x: 360.0,
			last_alpha: 0.0,
			build_info: create_build_info(),
		}
	}

	//==============================================================================
	// 로딩 화면 그리기 함수 만들기. (받은 수를 읽으려고 AssetLoader 손잡이를 나눠 든다)
	//==============================================================================
	fn create_loading_drawer(asset_loader: AssetLoader) -> Box<dyn FnMut(&mut Graphic, Vector2, f64, f64)> {
		Box::new(move |graphic: &mut Graphic, view_size: Vector2, progress: f64, elapsed_seconds: f64| {
			let center_x = view_size.x * 0.5;
			let center_y = view_size.y * 0.42;
			let orbit_radius = 56.0;
			graphic.set_stroke_color(Color::create_from_hex(ACCENT_COLOR).with_alpha(0.25));
			graphic.draw_stroke_circle(Vector2::create(center_x, center_y), orbit_radius, 3.0);
			for dot_index in 0..3 {
				let angle = elapsed_seconds * 2.4 + dot_index as f64 * std::f64::consts::PI * 2.0 / 3.0;
				let dot_position = Vector2::create(center_x + angle.cos() * orbit_radius, center_y + angle.sin() * orbit_radius);
				graphic.set_fill_color(ACCENT_COLOR);
				graphic.draw_circle(dot_position, 12.0);
			}
			let bar_width = 360.0;
			let bar_height = 12.0;
			let bar_left = center_x - bar_width * 0.5;
			let bar_top = center_y + 120.0;
			graphic.set_fill_color(Color::create(1.0, 1.0, 1.0, 0.15));
			graphic.draw_round_rect(Rect::create(bar_left, bar_top, bar_width, bar_height), bar_height * 0.5);
			let fill_width = bar_width * progress;
			if fill_width >= bar_height {
				graphic.set_fill_color(ACCENT_COLOR);
				graphic.draw_round_rect(Rect::create(bar_left, bar_top, fill_width, bar_height), bar_height * 0.5);
			}
			let status_text = format!("받는 중 {} / {}  ({}%)", asset_loader.get_finished_count(), asset_loader.get_total_count(), (progress * 100.0).round());
			graphic.set_font_string("20px default");
			graphic.set_text_align(TextAlign::Center);
			graphic.set_text_baseline(TextBaseline::Top);
			graphic.set_fill_color(Color::create(1.0, 1.0, 1.0, 0.85));
			graphic.draw_fill_text(&status_text, center_x, bar_top + 28.0);
		})
	}
}

impl Scene for LoadingShowcaseScene {
	fn get_scene_base(&self) -> &SceneBase {
		self.game_scene_base.get_scene_base()
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		self.game_scene_base.get_scene_base_mut()
	}

	//==============================================================================
	// 생성. (로딩 화면을 바꾸고, 받을 것을 넣고, 받기를 시작한다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.game_scene_base.set_view_scale_mode(ViewScaleMode::StretchShort, None);
		self.game_scene_base.set_loading_min_duration_ms(0.0);
		self.game_scene_base.set_loading_colors("#16324a", "#0b1a27", ACCENT_COLOR);
		self.game_scene_base.set_loading_outside_color("#05090d");
		let loading_drawer = LoadingShowcaseScene::create_loading_drawer(self.asset_loader.clone());
		self.game_scene_base.set_loading_drawer(Some(loading_drawer));
		self.game_scene_base.create(engine);

		self.asset_loader.set_concurrency(3);
		for image_name in IMAGE_NAME_LIST.iter() {
			let image_asset = self.asset_loader.add_image(image_name);
			self.image_asset_list.push(image_asset);
		}
		self.json_asset = Some(self.asset_loader.add_json("title.timeline.json"));
		self.image_asset_list.push(self.asset_loader.add_image("missing.png"));
		for stage_index in 0..STAGE_COUNT {
			let stage_handle = AssetLoadHandle::create_pending(&format!("stage{}", stage_index));
			self.asset_loader.add_load_handle(&stage_handle);
			self.stage_handle_list.push(stage_handle);
		}
		self.build_info.load(engine);
		if let Some(build_info_handle) = self.build_info.get_load_handle() {
			self.asset_loader.add_load_handle(build_info_handle);
		}
		self.game_scene_base.track_asset_loader(&self.asset_loader);
		self.asset_loader.load(engine);
	}

	//==============================================================================
	// 로딩 화면 출력. (10 프레임마다 흉내 단계 하나를 끝낸다)
	//==============================================================================
	fn draw_on_load(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.loading_frame_count += 1;
		let finished_stage_count = (self.loading_frame_count / STAGE_FRAME_INTERVAL) as usize;
		for stage_handle in self.stage_handle_list.iter().take(finished_stage_count) {
			if !stage_handle.is_finished() {
				stage_handle.finish(Ok(Vec::new()));
			}
		}
		self.game_scene_base.draw_on_load(engine, graphic);
	}

	//==============================================================================
	// 초기화.
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.game_scene_base.initialize(engine);
	}

	//==============================================================================
	// 화면 크기 변경됨.
	//==============================================================================
	fn resize(&mut self, engine: &mut Engine, canvas_native_size: Vector2) {
		self.game_scene_base.resize(engine, canvas_native_size);
	}

	//==============================================================================
	// 갱신. (공은 고정 걸음으로 움직인다. 벽에 닿으면 튕긴다)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.game_scene_base.tick(engine, time_delta);
		let ball_position_x = &mut self.ball_position_x;
		let previous_ball_position_x = &mut self.previous_ball_position_x;
		let ball_velocity_x = &mut self.ball_velocity_x;
		self.last_alpha = self.fixed_stepper.advance(time_delta, |step_seconds| {
			*previous_ball_position_x = *ball_position_x;
			*ball_position_x += *ball_velocity_x * step_seconds;
			if *ball_position_x > VIEW_WIDTH - 200.0 || *ball_position_x < 200.0 {
				*ball_velocity_x = -*ball_velocity_x;
			}
			true
		});
	}

	//==============================================================================
	// 이전 출력. (배경)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.game_scene_base.pre_draw(engine, graphic);
	}

	//==============================================================================
	// 출력. (받은 그림, 받은 수와 실패 목록, 고정 걸음 공)
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.set_fill_color("#16324a");
		graphic.draw_rect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT));
		graphic.set_font_string("24px default");
		graphic.set_text_align(TextAlign::Left);
		graphic.set_text_baseline(TextBaseline::Top);
		graphic.set_fill_color(Color::white());
		let summary_text = format!("AssetLoader: 받은 수 {} / 전체 {}, 실패 {}, 동시 {}", self.asset_loader.get_loaded_count(), self.asset_loader.get_total_count(), self.asset_loader.get_failed_list().len(), self.asset_loader.get_concurrency());
		graphic.draw_fill_text(&summary_text, 60.0, 50.0);

		let mut image_left = 60.0;
		for image_asset in self.image_asset_list.iter_mut() {
			let image = match image_asset.get_image() {
				Some(image) => {
					image
				}
				None => {
					continue;
				}
			};
			// 한 칸(140 x 120) 안에 비를 지켜 넣는다. (가로로 긴 스프라이트 시트도 칸을 넘지 않게)
			let box_width = 140.0;
			let box_height = 120.0;
			let image_width = image.get_width() as f64;
			let image_height = image.get_height() as f64;
			let fit_scale = (box_width / image_width.max(1.0)).min(box_height / image_height.max(1.0));
			let drawn_size = Vector2::create(image_width * fit_scale, image_height * fit_scale);
			let drawn_position = Vector2::create(image_left + (box_width - drawn_size.x) * 0.5, 110.0 + (box_height - drawn_size.y) * 0.5);
			graphic.draw_image(&image, drawn_position, drawn_size);
			image_left += box_width + 20.0;
		}

		graphic.set_font_string("20px default");
		let json_key_count = match self.json_asset.as_mut().and_then(|json_asset| json_asset.get_data()) {
			Some(serde_json::Value::Object(object_map)) => {
				object_map.len()
			}
			_ => {
				0
			}
		};
		graphic.draw_fill_text(&format!("title.timeline.json: 맨 위 키 {} 개", json_key_count), 60.0, 270.0);
		let build_text = self.build_info.read_text();
		graphic.draw_fill_text(&format!("BuildInfo: {}", build_text), 700.0, 270.0);
		let mut failure_top = 310.0;
		for failure in self.asset_loader.get_failed_list().iter() {
			graphic.set_fill_color("#ff8a80");
			graphic.draw_fill_text(&format!("실패: {}", failure.path), 60.0, failure_top);
			failure_top += 30.0;
		}

		let shown_ball_position_x = self.previous_ball_position_x + (self.ball_position_x - self.previous_ball_position_x) * self.last_alpha;
		graphic.set_fill_color("#0b1a27");
		graphic.draw_round_rect(Rect::create(180.0, 470.0, VIEW_WIDTH - 360.0, 80.0), 40.0);
		graphic.set_fill_color(ACCENT_COLOR);
		graphic.draw_circle(Vector2::create(shown_ball_position_x, 510.0), 28.0);
		graphic.set_fill_color(Color::white());
		graphic.draw_fill_text(&format!("FixedStepper: 1/30 초 걸음, 지난 프레임 {} 걸음, 남은 몫 {:.2}", self.fixed_stepper.get_last_step_count(), self.last_alpha), 180.0, 570.0);
	}
}

//==============================================================================
// 빌드 정보. (meta 도 version.json 도 없으면 이 크레이트의 판)
//==============================================================================
fn create_build_info() -> BuildInfo {
	let mut build_info = BuildInfo::new();
	build_info.set_package_version(env!("CARGO_PKG_VERSION"));
	build_info
}

//==============================================================================
// 진입점.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("로딩 쇼케이스 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(LoadingShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
