//==============================================================================
// misc 도구 둘러보기 예제. (데스크탑 창, 화면 확인용)
// - ImageScroller(흐르는 배경), Camera2D(천천히 좌우로 도는 회전) + Shaker(월드 격자와 PathFinder 경로), NodeLayout(패널), Typewriter + Localization(대사),
//   FloatingText(떠오르는 숫자), TouchEffect(빛 입자), SpriteAnimator(시트 애니메이션), VirtualPad(조이스틱)를 한 화면에 그린다.
// - 실행: cargo run --example misctour
//   검증용으로 스스로 끝내려면 VANILLA_CAPTURE_PATH=<폴더>/shot_{frame}.png VANILLA_CAPTURE_FRAMES=90 VANILLA_CAPTURE_EXIT=1 을 준다.
//==============================================================================

use std::rc::Rc;
use vanilla::{
	math, Camera2D, Color, Engine, EngineConfiguration, FloatingText, FloatingTextOptions, Graphic, Grid, GridCell, Image, ImageScroller, Localization, NodeId, NodeLayout, Paint, PathFinder, Pivot, Rect, Scene, SceneBase, Shaker,
	ShakerOptions, Sprite, SpriteAnimator, SpriteAnimatorPlayOptions, Text, TouchEffect, Typewriter, TypewriterOptions, Vector2, ViewScaleMode, VirtualPad,
};

// 화면 크기.
const VIEW_WIDTH: f64 = 960.0;
const VIEW_HEIGHT: f64 = 640.0;
// 월드 격자.
const GRID_COLUMN_COUNT: i32 = 12;
const GRID_ROW_COUNT: i32 = 8;
const TILE_SIZE: f64 = 48.0;
// 격자의 벽. ('#')
const GRID_MAP: [&str; 8] = [
	"............",
	"..####......",
	".....#..###.",
	".....#....#.",
	"..#..#....#.",
	"..#.......#.",
	"..#######.#.",
	"............",
];

//==============================================================================
// 격자 칸이 막혔는지.
//==============================================================================
fn is_wall(column: i32, row: i32) -> bool {
	GRID_MAP[row as usize].as_bytes()[column as usize] == b'#'
}

//==============================================================================
// 줄무늬 배경 그림을 만든다. (ImageScroller 용, 64x64)
//==============================================================================
fn create_stripe_image() -> Rc<Image> {
	let image_size = 64usize;
	let mut pixels = vec![0u8; image_size * image_size * 4];
	for pixel_y in 0..image_size {
		for pixel_x in 0..image_size {
			let is_stripe = ((pixel_x + pixel_y) / 16) % 2 == 0;
			let brightness = if is_stripe {
				48
			}
			else {
				36
			};
			let pixel_index = (pixel_y * image_size + pixel_x) * 4;
			pixels[pixel_index] = brightness;
			pixels[pixel_index + 1] = brightness;
			pixels[pixel_index + 2] = brightness + 16;
			pixels[pixel_index + 3] = 255;
		}
	}
	Image::create_from_pixels(image_size as u32, image_size as u32, pixels)
}

//==============================================================================
// 색 칸 네 개짜리 스프라이트 시트를 만든다. (SpriteAnimator 용, 128x32)
//==============================================================================
fn create_sheet_image() -> Rc<Image> {
	let frame_colors: [[u8; 3]; 4] = [[255, 120, 120], [255, 220, 120], [120, 230, 140], [120, 170, 255]];
	let image_width = 128usize;
	let image_height = 32usize;
	let mut pixels = vec![0u8; image_width * image_height * 4];
	for pixel_y in 0..image_height {
		for pixel_x in 0..image_width {
			let frame_color = frame_colors[pixel_x / 32];
			let local_x = (pixel_x % 32) as f64 - 15.5;
			let local_y = pixel_y as f64 - 15.5;
			let is_inside = local_x * local_x + local_y * local_y <= 14.0 * 14.0;
			let pixel_index = (pixel_y * image_width + pixel_x) * 4;
			if is_inside {
				pixels[pixel_index] = frame_color[0];
				pixels[pixel_index + 1] = frame_color[1];
				pixels[pixel_index + 2] = frame_color[2];
				pixels[pixel_index + 3] = 255;
			}
		}
	}
	Image::create_from_pixels(image_width as u32, image_height as u32, pixels)
}

//==============================================================================
// 둘러보기 씬.
//==============================================================================
struct MiscTourScene {
	scene_base: SceneBase,
	camera: Camera2D,
	shaker: Shaker,
	typewriter: Typewriter,
	virtual_pad: VirtualPad,
	image_scroller: ImageScroller,
	path: Vec<GridCell>,
	label_id: Option<NodeId>,
	floating_text_id: Option<NodeId>,
	touch_effect_id: Option<NodeId>,
	elapsed_seconds: f64,
	spawn_seconds: f64,
}

impl MiscTourScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> MiscTourScene {
		MiscTourScene {
			scene_base: SceneBase::new(),
			camera: Camera2D::new(),
			shaker: Shaker::new(ShakerOptions::default()),
			typewriter: Typewriter::new(TypewriterOptions {
				characters_per_second: 16.0,
				..TypewriterOptions::default()
			}),
			virtual_pad: VirtualPad::new(),
			image_scroller: ImageScroller::new(),
			path: Vec::new(),
			label_id: None,
			floating_text_id: None,
			touch_effect_id: None,
			elapsed_seconds: 0.0,
			spawn_seconds: 0.0,
		}
	}

	//==============================================================================
	// 월드 격자와 경로 그리기. (카메라 좌표계)
	//==============================================================================
	fn draw_world(&self, graphic: &mut Graphic) {
		for row in 0..GRID_ROW_COUNT {
			for column in 0..GRID_COLUMN_COUNT {
				let tile_rect = Rect::create(column as f64 * TILE_SIZE, row as f64 * TILE_SIZE, TILE_SIZE - 2.0, TILE_SIZE - 2.0);
				let tile_color = if is_wall(column, row) {
					Color::create(0.45, 0.32, 0.25, 1.0)
				}
				else {
					Color::create(0.18, 0.22, 0.3, 1.0)
				};
				graphic.set_fill_color(tile_color);
				graphic.draw_rect(tile_rect);
			}
		}
		let mut path_positions: Vec<Vector2> = vec![Grid::to_world_center(0, 7, TILE_SIZE)];
		for cell in self.path.iter() {
			let cell_center = Grid::to_world_center(cell.column, cell.row, TILE_SIZE);
			path_positions.push(cell_center);
		}
		graphic.set_stroke_color(Color::create(1.0, 0.85, 0.3, 1.0));
		graphic.draw_line(&path_positions, 6.0);
	}
}

impl Scene for MiscTourScene {
	//==============================================================================
	// 씬 공통 상태 반환.
	//==============================================================================
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	//==============================================================================
	// 씬 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (노드와 도구들)
	//==============================================================================
	fn create(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color(Color::create(0.08, 0.08, 0.12, 1.0));
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));

		// 흐르는 배경.
		self.image_scroller.set_image(Some(create_stripe_image()));
		self.image_scroller.set_view_rect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT));
		self.image_scroller.set_horizontal_scroll_speed(-40.0);
		self.image_scroller.set_vertical_scroll_speed(20.0);

		// 카메라와 길찾기.
		self.camera.set_view_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		self.camera.set_world_bounds(Some(Rect::create(0.0, 0.0, GRID_COLUMN_COUNT as f64 * TILE_SIZE, GRID_ROW_COUNT as f64 * TILE_SIZE)));
		self.camera.set_zoom(1.2);
		self.path = PathFinder::find_path(GRID_COLUMN_COUNT, GRID_ROW_COUNT, GridCell::create(0, 7), GridCell::create(11, 3), is_wall).unwrap_or_default();
		self.shaker.add_shake(6.0);

		// 대사 패널. (번역 표의 대사를 타자기로)
		Localization::set_table("en", [("{0}번째 misc 도구 둘러보기", "Misc tour number {0}: camera, path, dialogue")]);
		Localization::set_language("en");
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		NodeLayout::create()
			.name("panel")
			.pivot(Pivot::BOTTOM_CENTER)
			.content_size(620.0, 110.0)
			.local_position(VIEW_WIDTH * 0.5, VIEW_HEIGHT - 24.0)
			.component_with(Paint::new(), |paint| {
				paint.set_color(Color::create(0.0, 0.0, 0.0, 0.7));
				paint.set_round_size(16.0);
			})
			.children([NodeLayout::create().name("label").pivot(Pivot::TOP_LEFT).content_size(580.0, 90.0).local_position(20.0, 10.0).component_with(Text::new(), |text| {
				text.set_font_size(24.0);
				text.set_word_wrap_width(580.0);
			})])
			.build(node_tree, Some(root_id));
		let label_id = node_tree.find_descendant_by_name(root_id, "label");
		self.label_id = label_id;
		if let Some(label) = label_id.and_then(|label_id| node_tree[label_id].get_component_mut::<Text>()) {
			let dialogue_text = Localization::text("{0}번째 misc 도구 둘러보기", &[&1]);
			self.typewriter.start(label, Some(&dialogue_text));
		}

		// 시트 애니메이션.
		let sheet_image = create_sheet_image();
		let hero_id = node_tree.create_child(root_id, "hero");
		let hero_node = &mut node_tree[hero_id];
		hero_node.set_local_position(Vector2::create(120.0, 120.0));
		hero_node.set_content_size(Vector2::create(64.0, 64.0));
		hero_node.add_component(Sprite::new());
		let animator = hero_node.add_component(SpriteAnimator::new());
		let frame_rects: Vec<Rect> = (0..4).map(|frame_index| Rect::create(frame_index as f64 * 32.0, 0.0, 32.0, 32.0)).collect();
		animator.add_clip("spin", sheet_image, &frame_rects, 6.0, true);
		SpriteAnimator::play(hero_node, "spin", SpriteAnimatorPlayOptions::default());

		// 떠오르는 숫자와 빛 입자.
		self.floating_text_id = Some(FloatingText::create(node_tree, root_id));
		let touch_effect_id = node_tree.create_child(root_id, "TouchEffect");
		node_tree[touch_effect_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[touch_effect_id].add_component(TouchEffect::new());
		self.touch_effect_id = Some(touch_effect_id);

		// 조이스틱. (누른 채 오른쪽 위로 끈 모습)
		self.virtual_pad.set_pad_radius(70.0);
		self.virtual_pad.set_ball_radius(28.0);
		self.virtual_pad.set_pad_color(Color::create(1.0, 1.0, 1.0, 0.15));
		self.virtual_pad.set_ball_color(Color::create(1.0, 1.0, 1.0, 0.5));
		self.virtual_pad.set_pad_position(Vector2::create(VIEW_WIDTH - 120.0, VIEW_HEIGHT - 220.0));
		let pad_position = self.virtual_pad.get_pad_position();
		self.virtual_pad.touch_press(pad_position);
	}

	//==============================================================================
	// 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.elapsed_seconds += time_delta;
		self.spawn_seconds += time_delta;

		self.image_scroller.tick(time_delta);
		// 천천히 좌우로 돈다. (JS 예제 cameratour 와 같은 흔들기)
		let rotation = (self.elapsed_seconds * 0.6).sin() * 12.0;
		self.camera.set_rotation(rotation);
		let follow_target = Grid::to_world_center(11, 3, TILE_SIZE);
		self.camera.follow(&[follow_target], 1.5, time_delta);
		self.camera.tick(time_delta);
		self.shaker.tick(time_delta);

		// 조이스틱 구슬을 원을 그리며 끈다.
		let pad_position = self.virtual_pad.get_pad_position();
		let drag_angle = self.elapsed_seconds * 2.0 - 0.8;
		let drag_position = Vector2::create(pad_position.x + drag_angle.cos() * 120.0, pad_position.y + drag_angle.sin() * 120.0);
		self.virtual_pad.touch_move(drag_position);
		self.virtual_pad.tick(time_delta);

		let node_tree = self.scene_base.get_node_tree_mut();
		if let Some(label) = self.label_id.and_then(|label_id| node_tree[label_id].get_component_mut::<Text>()) {
			self.typewriter.tick(label, time_delta);
		}
		if self.spawn_seconds >= 0.2 {
			self.spawn_seconds = 0.0;
			let spawn_x = math::random_range(300.0, 660.0);
			let spawn_position = Vector2::create(spawn_x, 260.0);
			if let Some(floating_text_id) = self.floating_text_id {
				let damage_text = format!("-{}", (math::random() * 300.0).round());
				let floating_text_options = FloatingTextOptions {
					color: Color::create(1.0, 0.45, 0.45, 1.0),
					font_size: 28.0,
					..FloatingTextOptions::default()
				};
				FloatingText::spawn(node_tree, floating_text_id, &damage_text, spawn_position, floating_text_options);
			}
			if let Some(touch_effect) = self.touch_effect_id.and_then(|touch_effect_id| node_tree[touch_effect_id].get_component_mut::<TouchEffect>()) {
				for _ in 0..6 {
					touch_effect.create_touch_particle(spawn_x, 340.0);
				}
			}
		}
		self.scene_base.tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력. (배경색, 흐르는 줄무늬)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
		self.image_scroller.draw(graphic);
	}

	//==============================================================================
	// 출력. (카메라 + 흔들림으로 월드, 그 위에 노드)
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.push_state();
		let shake_offset = self.shaker.get_offset();
		graphic.translate(shake_offset.x, shake_offset.y);
		self.camera.begin(graphic);
		self.draw_world(graphic);
		self.camera.end(graphic);
		graphic.pop_state();
		self.scene_base.draw(graphic);
	}

	//==============================================================================
	// 이후 출력. (조이스틱)
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.virtual_pad.draw(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("misc tour (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(MiscTourScene::new())) {
		eprintln!("misctour: {}", error);
		std::process::exit(1);
	}
}
