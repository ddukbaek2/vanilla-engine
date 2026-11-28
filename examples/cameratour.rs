//==============================================================================
// 카메라 둘러보기 예제. (원본 examples/cameratour 와 같은 화면, 같은 동작)
// - 월드 격자를 Camera2D 로 비춘다. 카메라는 대상을 따라가며 천천히 좌우로 돌고(set_rotation), 가끔 흔들린다(Shaker).
// - 끌면 돌아간 화면 방향 그대로 팬하고(pan_by_screen), 휠은 커서 아래를 고정한 채 키운다(zoom_at).
// - 왼쪽 위: Tween::create_node_tween 이 노드 배율을 바로 움직인다(끝나면 반대로 다시). 오른쪽 위: Indicator.
// - 실행: cargo run --example cameratour
//   화면 저장: VANILLA_CAPTURE_PATH=shots/cameratour_{frame}.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1
//==============================================================================
use vanilla::{
	easing_function, Camera2D, Color, Engine, EngineConfiguration, Graphic, Indicator, NodeLayout, NodeId, Paint, Pivot, Rect, Scene, SceneBase, Shaker, ShakerOptions, Text, Tween, TweenId, TweenProperty, Vector2, ViewScaleMode,
};

// 화면 크기와 월드 격자.
const VIEW_WIDTH: f64 = 960.0;
const VIEW_HEIGHT: f64 = 640.0;
const TILE_SIZE: f64 = 48.0;
const GRID_COLUMN_COUNT: i32 = 24;
const GRID_ROW_COUNT: i32 = 16;

//==============================================================================
// 벽 칸인지. (격자 무늬로 벽을 흩어 둔다)
//==============================================================================
fn is_wall(column: i32, row: i32) -> bool {
	(column % 6 == 3 && row % 5 != 2) || (row % 7 == 5 && column % 4 == 1)
}

//==============================================================================
// 카메라 둘러보기 씬.
//==============================================================================
struct CameraTourScene {
	scene_base: SceneBase,
	camera: Camera2D,
	shaker: Shaker,
	indicator: Indicator,
	pulse_node_id: Option<NodeId>,
	pulse_tween_id: Option<TweenId>,
	pulse_start_scale: f64,
	pulse_end_scale: f64,
	drag_position: Option<Vector2>,
	elapsed_seconds: f64,
	shake_seconds: f64,
	is_following: bool,
}

impl CameraTourScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> CameraTourScene {
		CameraTourScene {
			scene_base: SceneBase::new(),
			camera: Camera2D::new(),
			shaker: Shaker::new(ShakerOptions::default()),
			indicator: Indicator::new(),
			pulse_node_id: None,
			pulse_tween_id: None,
			pulse_start_scale: 0.6,
			pulse_end_scale: 1.2,
			drag_position: None,
			elapsed_seconds: 0.0,
			shake_seconds: 0.0,
			is_following: true,
		}
	}

	//==============================================================================
	// 배율 트윈 시작. (끝나면 반대로 다시)
	//==============================================================================
	fn start_pulse(&mut self) {
		if let Some(pulse_node_id) = self.pulse_node_id {
			let tween = Tween::create_node_tween(pulse_node_id, TweenProperty::LocalScale, self.pulse_start_scale, self.pulse_end_scale, 0.8).easing(easing_function::sinusoidal::ease_in_out);
			self.pulse_tween_id = Some(self.scene_base.start_tween(tween));
		}
	}

	//==============================================================================
	// 월드 출력. (격자 칸과 노란 길)
	//==============================================================================
	fn draw_world(&self, graphic: &mut Graphic) {
		for row in 0..GRID_ROW_COUNT {
			for column in 0..GRID_COLUMN_COUNT {
				let tile_rect = Rect::create(column as f64 * TILE_SIZE, row as f64 * TILE_SIZE, TILE_SIZE - 2.0, TILE_SIZE - 2.0);
				if is_wall(column, row) {
					graphic.set_fill_color(Color::create(0.45, 0.32, 0.25, 1.0));
				}
				else {
					graphic.set_fill_color(Color::create(0.18, 0.22, 0.3, 1.0));
				}
				graphic.draw_rect(tile_rect);
			}
		}
		let path_positions = [
			Vector2::create(0.5 * TILE_SIZE, 7.5 * TILE_SIZE),
			Vector2::create(5.5 * TILE_SIZE, 7.5 * TILE_SIZE),
			Vector2::create(5.5 * TILE_SIZE, 2.5 * TILE_SIZE),
			Vector2::create(11.5 * TILE_SIZE, 2.5 * TILE_SIZE),
			Vector2::create(11.5 * TILE_SIZE, 3.5 * TILE_SIZE),
		];
		graphic.set_stroke_color(Color::create(1.0, 0.85, 0.3, 1.0));
		graphic.draw_line(&path_positions, 6.0);
	}
}

impl Scene for CameraTourScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 씬 구성.
	//==============================================================================
	fn load(&mut self, _engine: &Engine) {
		self.scene_base.set_scene_background_color(Color::create(20.0 / 255.0, 20.0 / 255.0, 31.0 / 255.0, 1.0));
		self.scene_base.get_root_mut().set_pivot(Pivot::TOP_LEFT);
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));

		self.camera.set_view_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		self.camera.set_world_bounds(Some(Rect::create(0.0, 0.0, GRID_COLUMN_COUNT as f64 * TILE_SIZE, GRID_ROW_COUNT as f64 * TILE_SIZE)));
		self.camera.set_zoom(1.2);

		self.indicator.set_position(VIEW_WIDTH - 60.0, 60.0);
		self.indicator.set_radius(26.0);
		self.indicator.set_color("#ffd34dff");

		// 배율이 숨 쉬듯 오가는 노드. (트윈이 노드에 바로 쓴다) 그 옆 설명 글.
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		NodeLayout::create()
			.name("Pulse")
			.content_size(56.0, 56.0)
			.local_position(70.0, 70.0)
			.component_with(Paint::new(), |paint| {
				paint.set_color(Color::create(0.35, 0.8, 1.0, 1.0));
				paint.set_round_size(12.0);
			})
			.build(node_tree, Some(root_id));
		NodeLayout::create()
			.name("Caption")
			.pivot(Pivot::TOP_LEFT)
			.content_size(720.0, 26.0)
			.local_position(130.0, 24.0)
			.component_with(Text::new(), |text| {
				text.set_text("Camera2D 회전: 끌면 팬, 휠은 커서 고정 줌");
				text.set_font_size(18.0);
				text.set_text_color(Color::create(0.9, 0.92, 1.0, 1.0));
			})
			.build(node_tree, Some(root_id));
		self.pulse_node_id = node_tree.find_descendant_by_name(root_id, "Pulse");
		self.start_pulse();
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		self.elapsed_seconds += time_delta;
		self.shake_seconds += time_delta;

		// 트윈이 끝났으면 반대로 다시.
		let is_pulse_running = self.pulse_tween_id.map(|tween_id| self.scene_base.get_tween(tween_id).is_some()).unwrap_or(false);
		if !is_pulse_running {
			std::mem::swap(&mut self.pulse_start_scale, &mut self.pulse_end_scale);
			self.start_pulse();
		}

		// 천천히 좌우로 돈다.
		let rotation = (self.elapsed_seconds * 0.6).sin() * 12.0;
		self.camera.set_rotation(rotation);
		let engine = Engine::get_engine().unwrap();
		self.camera.set_view_size(engine.get_view_manager().get_view_size());
		if self.is_following {
			let follow_target = Vector2::create(11.5 * TILE_SIZE, 3.5 * TILE_SIZE);
			self.camera.follow(&[follow_target], 1.5, time_delta);
		}
		self.camera.tick(time_delta);

		if self.shake_seconds >= 3.0 {
			self.shake_seconds = 0.0;
			self.shaker.add_shake(6.0);
		}
		self.shaker.tick(time_delta);
		self.indicator.tick(time_delta);
		self.scene_base.tick(time_delta);
	}

	//==============================================================================
	// 이전 출력. (바탕색)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
	}

	//==============================================================================
	// 출력. (흔들림 + 카메라로 월드, 그 위에 노드와 인디케이터)
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		graphic.push_state();
		let shake_offset = self.shaker.get_offset();
		graphic.translate(shake_offset.x, shake_offset.y);
		self.camera.begin(graphic);
		self.draw_world(graphic);
		self.camera.end(graphic);
		graphic.pop_state();
		self.scene_base.draw(graphic);
		self.indicator.draw(graphic);
	}

	//==============================================================================
	// 누름. (끌기 시작, 추적을 멈춘다)
	//==============================================================================
	fn touch_press(&mut self, view_input_position: Vector2) {
		self.drag_position = Some(view_input_position);
		self.is_following = false;
	}

	//==============================================================================
	// 끌기. (돌아간 화면 방향 그대로 팬)
	//==============================================================================
	fn touch_move(&mut self, view_input_position: Vector2) {
		if let Some(drag_position) = self.drag_position {
			self.camera.pan_by_screen(view_input_position.x - drag_position.x, view_input_position.y - drag_position.y);
			self.drag_position = Some(view_input_position);
		}
	}

	//==============================================================================
	// 놓음.
	//==============================================================================
	fn touch_release(&mut self, _view_input_position: Vector2) {
		self.drag_position = None;
	}

	//==============================================================================
	// 휠. (커서 아래 월드 지점을 고정한 채 줌)
	//==============================================================================
	fn touch_wheel(&mut self, view_input_position: Vector2, wheel_delta: Vector2) {
		let zoom_factor = if wheel_delta.y < 0.0 { 1.1 } else { 1.0 / 1.1 };
		self.camera.zoom_at(view_input_position, zoom_factor);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("camera tour (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.window_size = Vector2::create(1280.0, 720.0);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(CameraTourScene::new())) {
		eprintln!("cameratour: {}", error);
		std::process::exit(1);
	}
}
