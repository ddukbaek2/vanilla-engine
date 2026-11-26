//==============================================================================
// 글자, 노드, 그리기 예제. (출시 게임들이 따로 만들어 쓰던 것을 엔진 기본으로 넣은 것)
// - 글자 칸 맞춤: Text, UILabel 의 set_fit_width. 넘치면 최소 비율까지 줄이고, 그래도 넘치면 넘친 채 그린다. (테가 맞출 칸)
// - 노드 놓기: set_rect(왼쪽 위 기준, 피봇이 가운데여도), convert_view_to_local(마우스 자리를 노드 좌표로)
// - 그리기: Graphic.draw_ring(반투명 띠), draw_arc(재사용 대기 고리처럼 줄어드는 호), draw_line_segments(선분 수백 개를 한 번에)
// - 아틀라스: tools/atlas.cjs 가 구운 examples/assets/atlas/icons.json 을 SpriteAtlas 로 읽어 조각을 스프라이트에 건다
// - 그림자 글 셋 가운데 같은 둘은 구운 그림을 나눠 쓴다. (sharedbakecache)
// - 실행: cargo run --example textnodeextras
//==============================================================================
use std::path::PathBuf;
use vanilla::{math, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, NodeId, NodeTransform, NodeTree, Outline, Paint, Pivot, Scene, SceneBase, ShadowText, Sprite, SpriteAtlas, Text, TextAlign, TextBaseline, UILabel, Vector2};

const EXAMPLE_ASSET_DIRECTORY: &str = "examples/assets";
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const BACKGROUND_COLOR: &str = "#151821";
const TEXT_COLOR: &str = "#e8ebf2";
const CAPTION_COLOR: &str = "#9aa2b2";
const BOX_COLOR: &str = "#262b38";
const FIT_LINE_COLOR: &str = "#6d7dff";
const ATLAS_FRAME_NAMES: [&str; 5] = ["star", "heart", "leaf", "gem", "coin"];

//==============================================================================
// 상자 노드 만들기. (왼쪽 위 피봇, set_rect)
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, x: f64, y: f64, width: f64, height: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_rect(x, y, width, height);
	node_id
}

//==============================================================================
// 설명 글 노드 만들기.
//==============================================================================
fn create_caption_node(node_tree: &mut NodeTree, parent_id: NodeId, x: f64, y: f64, text: &str) -> NodeId {
	let caption_id = create_box_node(node_tree, parent_id, "caption", x, y, 560.0, 24.0);
	let caption_text = node_tree[caption_id].add_component(Text::create_with_text(text, 16.0));
	caption_text.set_text_color(CAPTION_COLOR);
	caption_text.set_text_align(TextAlign::Left);
	caption_text.set_text_baseline(TextBaseline::Middle);
	caption_id
}

//==============================================================================
// 호와 고리, 선분을 그리는 컴포넌트. (노드 내용 영역 왼쪽 위가 원점)
//==============================================================================
struct ShapeGallery {
	component_base: ComponentBase,
	elapsed_seconds: f64,
	line_position_values: Vec<f64>,
}

#[vanilla::reflect]
impl ShapeGallery {
	//==============================================================================
	// 생성. (선분은 한 번 지어 두고 그릴 때마다 그대로 넘긴다)
	//==============================================================================
	fn new() -> ShapeGallery {
		let mut line_position_values = Vec::new();
		for line_index in 0..600 {
			let column = (line_index % 30) as f64;
			let row = (line_index / 30) as f64;
			let x = 360.0 + column * 9.0;
			let y = 10.0 + row * 9.0;
			line_position_values.extend_from_slice(&[x, y, x + 6.0, y + 6.0]);
		}
		ShapeGallery {
			component_base: ComponentBase::new(),
			elapsed_seconds: 0.0,
			line_position_values,
		}
	}
}

#[vanilla::reflect]
impl Component for ShapeGallery {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"ShapeGallery"
	}

	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f64) {
		self.elapsed_seconds += time_delta;
	}

	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		// 반투명 고리 둘이 겹친다. (조각 이음매가 짙어지지 않는다)
		graphic.set_fill_color("rgba(255, 90, 90, 0.55)");
		graphic.draw_ring(Vector2::create(70.0, 100.0), 40.0, 62.0, 0.0, math::PI * 2.0);
		graphic.set_fill_color("rgba(80, 200, 255, 0.55)");
		graphic.draw_ring(Vector2::create(115.0, 100.0), 40.0, 62.0, 0.0, math::PI * 2.0);

		// 재사용 대기처럼 12 시에서 시계 방향으로 줄어드는 호와 바탕 고리.
		let remain_ratio = 1.0 - (self.elapsed_seconds * 0.25).fract();
		graphic.set_fill_color("rgba(255, 255, 255, 0.15)");
		graphic.draw_ring(Vector2::create(260.0, 100.0), 46.0, 56.0, 0.0, math::PI * 2.0);
		graphic.set_stroke_color("#ffd166");
		graphic.draw_arc(Vector2::create(260.0, 100.0), 51.0, -math::PI * 0.5, -math::PI * 0.5 + math::PI * 2.0 * remain_ratio, 10.0);

		// 부채꼴 띠. (안쪽 반지름 0)
		graphic.set_fill_color("rgba(120, 255, 160, 0.6)");
		graphic.draw_ring(Vector2::create(70.0, 230.0), 0.0, 50.0, -math::PI * 0.5, math::PI * 0.75);
		graphic.set_fill_color("#ff9f43");
		graphic.draw_ring(Vector2::create(200.0, 230.0), 20.0, 50.0, math::PI, math::PI * 1.75);

		// 선분 600 개를 숫자 배열 하나로.
		graphic.set_stroke_color("rgba(140, 255, 140, 0.85)");
		graphic.draw_line_segments(&self.line_position_values, 1.5);
	}
}

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	scene_base: SceneBase,
	sprite_atlas: SpriteAtlas,
	sprite_node_ids: Vec<NodeId>,
	is_atlas_applied: bool,
	pointer_label_id: Option<NodeId>,
	pointer_target_id: Option<NodeId>,
	pulse_label_id: Option<NodeId>,
	elapsed_seconds: f64,
}

impl MainScene {
	//==============================================================================
	// 글자 칸 맞춤 칸. (같은 너비의 칸 넷, 테가 맞출 칸)
	//==============================================================================
	fn create_fit_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_caption_node(node_tree, root_id, 40.0, 90.0, "set_fit_width (칸 220, 최소 비율 0.55)");
		let text_list = ["짧은 글", "조금 길어서 줄여야 맞는 글", "아주아주 길어서 최소 비율까지 줄여도 넘치는 글입니다", "UILabel 도 같은 맞춤을 씁니다"];
		for (text_index, text) in text_list.iter().enumerate() {
			let y = 124.0 + text_index as f64 * 56.0;
			let box_id = create_box_node(node_tree, root_id, "fitBox", 40.0, y, 220.0, 44.0);
			let box_node = &mut node_tree[box_id];
			let paint = box_node.add_component(Paint::create_with_color(BOX_COLOR));
			paint.set_round_size(8.0);
			let outline = box_node.add_component(Outline::new());
			outline.set_color(FIT_LINE_COLOR);
			outline.set_line_width(1.0);
			outline.set_round_size(8.0);
			let label_id = create_box_node(node_tree, box_id, "fitText", 0.0, 0.0, 220.0, 44.0);
			if text_index == 3 {
				let label = node_tree.add_component(label_id, UILabel::create_with_text(text, 24.0)).expect("label");
				label.set_text_color(TEXT_COLOR);
				label.set_fit_width(220.0, 0.55);
			}
			else {
				let fit_text = node_tree[label_id].add_component(Text::create_with_text(text, 24.0));
				fit_text.set_text_color(TEXT_COLOR);
				fit_text.set_fit_width(220.0, 0.55);
			}
		}

		// 배율이 움직이는 글. (굽기 배율 단계가 0 이어도 움직이는 동안은 묶은 배율로 굽는다)
		let pulse_id = create_box_node(node_tree, root_id, "pulse", 40.0, 360.0, 220.0, 44.0);
		node_tree[pulse_id].set_pivot(Pivot::MIDDLE_CENTER);
		node_tree[pulse_id].set_rect(40.0, 360.0, 220.0, 44.0);
		let pulse_text = node_tree[pulse_id].add_component(Text::create_with_text("커졌다 작아지는 글", 24.0));
		pulse_text.set_text_color("#ffd166");
		self.pulse_label_id = Some(pulse_id);
	}

	//==============================================================================
	// 호와 고리, 선분 칸.
	//==============================================================================
	fn create_shape_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_caption_node(node_tree, root_id, 340.0, 90.0, "draw_ring, draw_arc, draw_line_segments");
		let gallery_id = create_box_node(node_tree, root_id, "shapes", 340.0, 124.0, 640.0, 300.0);
		node_tree[gallery_id].add_component(ShapeGallery::new());
	}

	//==============================================================================
	// 아틀라스 칸. (조각은 아틀라스가 다 읽힌 뒤 tick 에서 건다)
	//==============================================================================
	fn create_atlas_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_caption_node(node_tree, root_id, 40.0, 440.0, "SpriteAtlas (atlas.cjs, 늘여 붙이기 2)");
		for frame_index in 0..ATLAS_FRAME_NAMES.len() {
			let sprite_id = create_box_node(node_tree, root_id, "atlasSprite", 40.0 + frame_index as f64 * 76.0, 474.0, 64.0, 64.0);
			node_tree[sprite_id].add_component(Sprite::new());
			self.sprite_node_ids.push(sprite_id);
		}

		// 같은 모양의 그림자 글 둘은 구운 그림 하나를 나눠 쓴다.
		create_caption_node(node_tree, root_id, 460.0, 440.0, "ShadowText (위 둘은 구운 그림 하나)");
		for (shadow_index, text) in ["같은 그림자 글", "같은 그림자 글", "다른 그림자 글"].iter().enumerate() {
			let shadow_id = create_box_node(node_tree, root_id, "shadowText", 460.0, 474.0 + shadow_index as f64 * 40.0, 300.0, 36.0);
			let shadow_text = node_tree[shadow_id].add_component(ShadowText::create_with_text(text, 24.0));
			shadow_text.set_text_style(24.0, true, TEXT_COLOR);
			shadow_text.set_shadow("#000000", 2.0, 3.0);
		}
	}

	//==============================================================================
	// 뷰 좌표 → 노드 좌표 칸. (마우스가 올라간 자리를 돌아간 노드의 좌표로)
	//==============================================================================
	fn create_pointer_section(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		create_caption_node(node_tree, root_id, 820.0, 440.0, "convert_view_to_local (돌아간 판)");
		let target_id = node_tree.create_child(root_id, "pointerTarget");
		let target_node = &mut node_tree[target_id];
		target_node.set_pivot(Pivot::MIDDLE_CENTER);
		target_node.set_rect(860.0, 490.0, 240.0, 120.0);
		target_node.set_local_rotation(12.0);
		let paint = target_node.add_component(Paint::create_with_color("#2f3a55"));
		paint.set_round_size(10.0);
		let label_id = create_box_node(node_tree, target_id, "pointerLabel", 0.0, 0.0, 240.0, 120.0);
		let label_text = node_tree[label_id].add_component(Text::create_with_text("마우스를 올려 보세요", 18.0));
		label_text.set_text_color(TEXT_COLOR);
		self.pointer_target_id = Some(target_id);
		self.pointer_label_id = Some(label_id);
	}

	//==============================================================================
	// 아틀라스가 다 읽혔으면 조각 걸기. (데스크탑은 처음 tick 에, 웹은 쪽 그림이 온 뒤)
	//==============================================================================
	fn apply_atlas_frames(&mut self, engine: &Engine) {
		if self.is_atlas_applied || !self.sprite_atlas.update(engine) {
			return;
		}
		self.is_atlas_applied = true;
		let node_tree = self.scene_base.get_node_tree_mut();
		for (frame_index, sprite_id) in self.sprite_node_ids.iter().enumerate() {
			if let Some(sprite) = node_tree[*sprite_id].get_component_mut::<Sprite>() {
				self.sprite_atlas.apply_frame(sprite, ATLAS_FRAME_NAMES[frame_index]);
			}
		}
	}
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (아틀라스 JSON 을 씬이 기다리게 맡긴다)
	//==============================================================================
	fn create(&mut self, engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		self.sprite_atlas.load(engine, "atlas/icons.json");
		self.sprite_atlas.track_json_asset(&mut self.scene_base);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_box_node(node_tree, root_id, "title", 40.0, 30.0, 900.0, 50.0);
		let title_text = node_tree[title_id].add_component(Text::create_with_text("글자, 노드, 그리기", 34.0));
		title_text.set_text_color(TEXT_COLOR);
		title_text.set_text_align(TextAlign::Left);
		self.create_fit_section(root_id);
		self.create_shape_section(root_id);
		self.create_atlas_section(root_id);
		self.create_pointer_section(root_id);
	}

	//==============================================================================
	// 초기화. (데스크탑은 아틀라스가 이미 다 읽혀 여기서 조각을 건다)
	//==============================================================================
	fn initialize(&mut self, engine: &mut Engine) {
		self.apply_atlas_frames(engine);
	}

	//==============================================================================
	// 주기적 갱신.
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		self.elapsed_seconds += time_delta;
		self.apply_atlas_frames(engine);
		let pulse_scale = 1.0 + 0.15 * (self.elapsed_seconds * 2.0).sin().max(0.0);
		let view_input_position = engine.get_input_manager().get_view_input_position();
		let node_tree = self.scene_base.get_node_tree_mut();
		if let Some(pulse_id) = self.pulse_label_id {
			node_tree[pulse_id].set_local_scale_xy(pulse_scale, pulse_scale);
		}
		if let (Some(target_id), Some(label_id)) = (self.pointer_target_id, self.pointer_label_id) {
			let local_position = node_tree.convert_view_to_local(target_id, view_input_position);
			let content_size = node_tree[target_id].get_content_size();
			let label = match local_position {
				Some(local_position) if local_position.x >= 0.0 && local_position.y >= 0.0 && local_position.x <= content_size.x && local_position.y <= content_size.y => {
					format!("판 안 ({:.0}, {:.0})", local_position.x, local_position.y)
				}
				_ => {
					String::from("마우스를 올려 보세요")
				}
			};
			if let Some(label_text) = node_tree[label_id].get_component_mut::<Text>() {
				label_text.set_text(&label);
			}
		}
		self.scene_base.tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력. (글을 1:1 로 굽게 굽기 배율 단계를 0 으로 두고, 배경을 칠하고 기준 해상도 좌표계를 건다, 게임들과 같다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		graphic.set_text_bake_scale_step(0.0);
		self.scene_base.draw_scene_background(engine, graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("글자, 노드, 그리기 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
		sprite_atlas: SpriteAtlas::new(),
		sprite_node_ids: Vec::new(),
		is_atlas_applied: false,
		pointer_label_id: None,
		pointer_target_id: None,
		pulse_label_id: None,
		elapsed_seconds: 0.0,
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
