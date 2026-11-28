//==============================================================================
// 파티클 이펙트 예제. (원본 examples/particleeffects 와 같은 장면)
// - 왼쪽 목록에서 이펙트(불, 분수, 눈, 비, 색종이, 폭발, 불꽃놀이, 별가루, 불씨, 거품, 소용돌이, 마법 궤적, 유성우, 번개, 은하, 군집, 화산, 워프) 열여덟 가지를 고르면
//   오른쪽 둥근 무대에서 무한 반복된다. 무대 위 왼쪽에는 사용 코드 패널(COPY 단추로 클립보드에 복사), 오른쪽 아래에는 fps, 입자, 정점, 삼각형 수가 뜬다.
// - 색종이, 폭발, 불꽃놀이, 번개, 화산은 일정 간격으로 자동 분출하고, 마법 궤적, 은하, 군집은 매 틱 노드나 끌개를 움직인다.
// - 실행: cargo run --release --example particleeffects
//   처음 모드 고르기: VANILLA_EXAMPLE_MODE=galaxy cargo run --release --example particleeffects
//   화면 저장: VANILLA_CAPTURE_PATH=shots/particleeffects.png VANILLA_CAPTURE_FRAMES=150 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::cell::Cell;
use std::rc::Rc;
use vanilla::core::graphic::{TextAlign, TextBaseline};
use vanilla::{
	Color, ComponentContext, Engine, EngineConfiguration, Graphic, Mask, NodeId, NodeTree, Paint, ParticleEmitterShape, ParticleRenderShape, ParticleSystem, Pivot, Scene, SceneBase, SeededRandom, TouchRecognizer, TrailRenderer, UIButton, UILabel, UIScrollBar, UIScrollView, Vector2, ViewScaleMode,
};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const REFERENCE_WIDTH: f64 = 1280.0;
const REFERENCE_HEIGHT: f64 = 760.0;
const LIST_X: f64 = 24.0;
const LIST_Y: f64 = 78.0;
const LIST_WIDTH: f64 = 248.0;
const LIST_HEIGHT: f64 = 656.0;
const LIST_ROW_HEIGHT: f64 = 46.0;
const STAGE_X: f64 = 288.0;
const STAGE_Y: f64 = 78.0;
const STAGE_WIDTH: f64 = 968.0;
const STAGE_HEIGHT: f64 = 656.0;
const EFFECT_RANDOM_SEED: u32 = 20260901;
const MODE_FIRE: usize = 0;
const MODE_CONFETTI: usize = 4;
const MODE_EXPLOSION: usize = 5;
const MODE_FIREWORK: usize = 6;
const MODE_TRAIL: usize = 11;
const MODE_LIGHTNING: usize = 13;
const MODE_GALAXY: usize = 14;
const MODE_SWARM: usize = 15;
const MODE_VOLCANO: usize = 16;

fn text_color() -> Color {
	Color::create(0.91, 0.922, 0.949, 1.0)
}

fn text_dim_color() -> Color {
	Color::create(0.604, 0.635, 0.698, 1.0)
}

fn text_faint_color() -> Color {
	Color::create(0.42, 0.451, 0.529, 1.0)
}

fn on_accent_color() -> Color {
	Color::create(0.165, 0.126, 0.086, 1.0)
}

fn accent_color() -> Color {
	Color::create(0.831, 0.69, 0.416, 1.0)
}

fn transparent_color() -> Color {
	Color::create(0.0, 0.0, 0.0, 0.0)
}

//==============================================================================
// 이펙트 모드 정의. (키, 제목, 꼬리표, 무대에 보이는 사용 코드)
//==============================================================================
struct ModeDefinition {
	key: &'static str,
	title: &'static str,
	tag: &'static str,
	usage: &'static [&'static str],
}

const MODE_DEFINITIONS: &[ModeDefinition] = &[
	ModeDefinition {
		key: "fire",
		title: "FIRE",
		tag: "cone, additive",
		usage: &[
			"let flame = node.add_component(ParticleSystem::new());",
			"flame.set_emitter_shape(ParticleEmitterShape::Cone);",
			"flame.set_cone_angle(0.34);",
			"flame.set_additive(true);",
			"flame.set_gravity(0.0, -130.0);",
			"flame.set_color_over_lifetime(vec![(0.0, yellow), (1.0, dark_red)]);",
		],
	},
	ModeDefinition {
		key: "fountain",
		title: "FOUNTAIN",
		tag: "gravity",
		usage: &[
			"let water = node.add_component(ParticleSystem::new());",
			"water.set_emitter_shape(ParticleEmitterShape::Cone);",
			"water.set_cone_angle(0.22);",
			"water.set_start_speed(420.0, 560.0);",
			"water.set_gravity(0.0, 460.0);",
			"water.set_size_over_lifetime(1.0, 0.6);",
		],
	},
	ModeDefinition {
		key: "snow",
		title: "SNOW",
		tag: "edge, wobble",
		usage: &[
			"let snow = node.add_component(ParticleSystem::new());",
			"snow.set_emitter_shape(ParticleEmitterShape::Edge);",
			"snow.set_edge_width(STAGE_WIDTH * 1.1);",
			"snow.set_start_lifetime(7.0, 11.0);",
			"snow.set_wobble(16.0, 1.4);",
		],
	},
	ModeDefinition {
		key: "rain",
		title: "RAIN",
		tag: "streak",
		usage: &[
			"let rain = node.add_component(ParticleSystem::new());",
			"rain.set_emitter_shape(ParticleEmitterShape::Edge);",
			"rain.set_render_shape(ParticleRenderShape::Streak);",
			"rain.set_streak_scale(0.03);",
			"rain.set_start_speed(500.0, 620.0);",
			"rain.set_emission_rate(520.0);",
		],
	},
	ModeDefinition {
		key: "confetti",
		title: "CONFETTI",
		tag: "burst, rect",
		usage: &[
			"let confetti = node.add_component(ParticleSystem::new());",
			"confetti.set_render_shape(ParticleRenderShape::Rect);",
			"confetti.set_angular_velocity(1.6, 3.2);",
			"confetti.set_gravity(0.0, 460.0);",
			"confetti.emit(40, Some(burst_position));",
		],
	},
	ModeDefinition {
		key: "explosion",
		title: "EXPLOSION",
		tag: "burst, mixed",
		usage: &[
			"let sparks = node.add_component(ParticleSystem::new());",
			"sparks.set_render_shape(ParticleRenderShape::Streak);",
			"sparks.set_streak_scale(0.045);",
			"sparks.set_additive(true);",
			"sparks.set_damping(2.2);",
			"sparks.emit(110, Some(position));",
		],
	},
	ModeDefinition {
		key: "firework",
		title: "FIREWORK",
		tag: "burst, additive",
		usage: &[
			"let burst = node.add_component(ParticleSystem::new());",
			"burst.set_emitter_shape(ParticleEmitterShape::Circle);",
			"burst.set_shape_radius(4.0);",
			"burst.set_additive(true);",
			"burst.set_gravity(0.0, 150.0);",
			"burst.emit(130, Some(sky_position));",
		],
	},
	ModeDefinition {
		key: "sparkle",
		title: "SPARKLE",
		tag: "box, soft disc",
		usage: &[
			"let sparkle = node.add_component(ParticleSystem::new());",
			"sparkle.set_emitter_shape(ParticleEmitterShape::Box);",
			"sparkle.set_box_size(STAGE_WIDTH * 0.94, STAGE_HEIGHT * 0.86);",
			"sparkle.set_additive(true);",
			"sparkle.set_size_over_lifetime(0.4, 1.3);",
		],
	},
	ModeDefinition {
		key: "embers",
		title: "EMBERS",
		tag: "rise, wobble",
		usage: &[
			"let embers = node.add_component(ParticleSystem::new());",
			"embers.set_emitter_shape(ParticleEmitterShape::Edge);",
			"embers.set_gravity(0.0, -85.0);",
			"embers.set_wobble(18.0, 1.8);",
			"embers.set_additive(true);",
		],
	},
	ModeDefinition {
		key: "bubbles",
		title: "BUBBLES",
		tag: "rise, sway",
		usage: &[
			"let bubbles = node.add_component(ParticleSystem::new());",
			"bubbles.set_emitter_shape(ParticleEmitterShape::Edge);",
			"bubbles.set_gravity(0.0, -75.0);",
			"bubbles.set_wobble(22.0, 2.4);",
			"bubbles.set_size_over_lifetime(0.7, 1.15);",
		],
	},
	ModeDefinition {
		key: "vortex",
		title: "VORTEX",
		tag: "attractor",
		usage: &[
			"let vortex = node.add_component(ParticleSystem::new());",
			"vortex.set_emitter_shape(ParticleEmitterShape::Circle);",
			"vortex.set_shape_radius(280.0);",
			"vortex.set_attractor(0.0, 0.0, 190.0, 260.0);",
			"vortex.set_damping(0.35);",
		],
	},
	ModeDefinition {
		key: "trail",
		title: "TRAIL",
		tag: "TrailRenderer",
		usage: &[
			"let trail = node.add_component(TrailRenderer::new());",
			"trail.set_time(0.65);",
			"trail.set_width(20.0, 0.0);",
			"trail.set_additive(true);",
			"trail.set_colors(start_color, end_color);",
		],
	},
	ModeDefinition {
		key: "meteor",
		title: "METEOR",
		tag: "streak, diagonal",
		usage: &[
			"let meteor = node.add_component(ParticleSystem::new());",
			"meteor.set_emitter_shape(ParticleEmitterShape::Edge);",
			"meteor.set_render_shape(ParticleRenderShape::Streak);",
			"meteor.set_streak_scale(0.09);",
			"meteor.set_gravity(420.0, 340.0);",
		],
	},
	ModeDefinition {
		key: "lightning",
		title: "LIGHTNING",
		tag: "path emit, flash",
		usage: &[
			"let bolt = node.add_component(ParticleSystem::new());",
			"bolt.set_start_lifetime(0.2, 0.45);",
			"bolt.set_start_speed(0.0, 8.0);",
			"bolt.set_additive(true);",
			"// every strike: walk a zigzag path and stamp particles along it",
			"bolt.emit(3, Some(Vector2::create(path_x, path_y)));",
		],
	},
	ModeDefinition {
		key: "galaxy",
		title: "GALAXY",
		tag: "orbiting emitter",
		usage: &[
			"let arm = arm_node.add_component(ParticleSystem::new());",
			"arm.set_world_space(true);",
			"arm.set_start_lifetime(2.2, 3.6);",
			"arm.set_start_speed(3.0, 16.0);",
			"// every tick: orbit the emitter node around the core",
			"arm_node.set_local_position(Vector2::create(center_x + angle.cos() * radius, center_y + angle.sin() * radius));",
		],
	},
	ModeDefinition {
		key: "swarm",
		title: "SWARM",
		tag: "moving attractor",
		usage: &[
			"let swarm = node.add_component(ParticleSystem::new());",
			"swarm.set_damping(0.5);",
			"swarm.set_wobble(6.0, 6.0);",
			"// every tick: chase a target that keeps running away",
			"swarm.set_attractor(target_x, target_y, 620.0, 140.0);",
		],
	},
	ModeDefinition {
		key: "volcano",
		title: "VOLCANO",
		tag: "burst, scene",
		usage: &[
			"let lava = node.add_component(ParticleSystem::new());",
			"lava.set_emitter_shape(ParticleEmitterShape::Cone);",
			"lava.set_start_speed(320.0, 560.0);",
			"lava.set_gravity(0.0, 640.0);",
			"// eruption every 1.7 seconds",
			"lava.emit(26, None);",
		],
	},
	ModeDefinition {
		key: "warp",
		title: "WARP",
		tag: "radial streak",
		usage: &[
			"let warp = node.add_component(ParticleSystem::new());",
			"warp.set_emitter_shape(ParticleEmitterShape::Circle);",
			"warp.set_shape_radius(26.0);",
			"warp.set_render_shape(ParticleRenderShape::Streak);",
			"warp.set_streak_scale(0.1);",
			"warp.set_start_speed(480.0, 880.0);",
		],
	},
];

//==============================================================================
// 목록 한 줄이 쥐는 노드.
//==============================================================================
struct ListRow {
	paint_node_id: NodeId,
	title_node_id: NodeId,
	tag_node_id: NodeId,
}

//==============================================================================
// 왼쪽 위 기준 상자 노드 만들기.
//==============================================================================
fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, x: f64, y: f64, width: f64, height: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, name);
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_content_size(Vector2::create(width, height));
	node.set_local_position(Vector2::create(x, y));
	node_id
}

//==============================================================================
// 글자 노드 만들기. (UILabel, 왼쪽 위 기준)
//==============================================================================
fn create_label_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, x: f64, y: f64, width: f64, height: f64, font_size: f64, color: Color, is_bold: bool) -> NodeId {
	let label_id = create_box_node(node_tree, parent_id, name, x, y, width, height);
	let label = node_tree.add_component(label_id, UILabel::new()).expect("label");
	label.set_font_size(font_size);
	label.set_text_color(color);
	label.set_text_align(TextAlign::Left);
	if is_bold {
		label.set_bold(true);
	}
	label_id
}

//==============================================================================
// 노드의 UILabel 문구와 색 바꾸기.
//==============================================================================
fn set_label_text(node_tree: &mut NodeTree, label_id: NodeId, text: &str) {
	if let Some(label) = node_tree[label_id].get_component_mut::<UILabel>() {
		label.set_text(text);
	}
}

fn set_label_color(node_tree: &mut NodeTree, label_id: NodeId, color: Color) {
	if let Some(label) = node_tree[label_id].get_component_mut::<UILabel>() {
		label.set_text_color(color);
	}
}

//==============================================================================
// 파티클 이펙트 씬.
//==============================================================================
struct ParticleEffectsScene {
	scene_base: SceneBase,
	touch_recognizer: TouchRecognizer,
	random: SeededRandom,
	mode_index: usize,
	// 모드마다: 뿌리 노드, 파티클 시스템이 붙은 노드들.
	mode_root_ids: Vec<NodeId>,
	mode_system_ids: Vec<Vec<NodeId>>,
	list_rows: Vec<ListRow>,
	stage_id: Option<NodeId>,
	stage_title_id: Option<NodeId>,
	stage_usage_id: Option<NodeId>,
	usage_panel_id: Option<NodeId>,
	copy_button_id: Option<NodeId>,
	copy_label_id: Option<NodeId>,
	copy_reset_seconds: f64,
	fps_label_id: Option<NodeId>,
	count_label_id: Option<NodeId>,
	vertex_label_id: Option<NodeId>,
	triangle_label_id: Option<NodeId>,
	trail_node_id: Option<NodeId>,
	galaxy_arm_ids: Vec<NodeId>,
	burst_accumulator: f64,
	burst_cursor: usize,
	elapsed_seconds: f64,
	average_delta: f64,
	// 단추 눌림은 여기로 알린다. (선택한 모드, 복사 눌림)
	requested_mode: Rc<Cell<Option<usize>>>,
	is_copy_requested: Rc<Cell<bool>>,
}

impl ParticleEffectsScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> ParticleEffectsScene {
		ParticleEffectsScene {
			scene_base: SceneBase::new(),
			touch_recognizer: TouchRecognizer::new(),
			random: SeededRandom::new(EFFECT_RANDOM_SEED),
			mode_index: MODE_FIRE,
			mode_root_ids: Vec::new(),
			mode_system_ids: Vec::new(),
			list_rows: Vec::new(),
			stage_id: None,
			stage_title_id: None,
			stage_usage_id: None,
			usage_panel_id: None,
			copy_button_id: None,
			copy_label_id: None,
			copy_reset_seconds: 0.0,
			fps_label_id: None,
			count_label_id: None,
			vertex_label_id: None,
			triangle_label_id: None,
			trail_node_id: None,
			galaxy_arm_ids: Vec::new(),
			burst_accumulator: 0.0,
			burst_cursor: 0,
			elapsed_seconds: 0.0,
			average_delta: 0.0,
			requested_mode: Rc::new(Cell::new(None)),
			is_copy_requested: Rc::new(Cell::new(false)),
		}
	}

	//==============================================================================
	// 왼쪽 이펙트 목록. (스크롤 목록, 선택 행은 액센트로 칠한다)
	//==============================================================================
	fn build_effect_list(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let list_id = create_box_node(node_tree, root_id, "EffectList", LIST_X, LIST_Y, LIST_WIDTH, LIST_HEIGHT);
		let list_paint = node_tree[list_id].add_component(Paint::create_with_color(Color::create(0.106, 0.125, 0.165, 1.0)));
		list_paint.set_round_size(12.0);
		let content_id = {
			let list_scroll = node_tree.add_component(list_id, UIScrollView::new()).expect("scroll view");
			list_scroll.set_horizontal(false);
			list_scroll.set_vertical(true);
			list_scroll.set_scroll_content_size(Vector2::create(LIST_WIDTH, MODE_DEFINITIONS.len() as f64 * LIST_ROW_HEIGHT + 16.0));
			list_scroll.get_content().expect("content")
		};

		// 스크롤바. (스크롤 뷰 노드의 형제로 두어야 끌기가 스크롤 뷰로 넘어가지 않는다)
		let scroll_bar_id = create_box_node(node_tree, root_id, "EffectListScrollBar", LIST_X + LIST_WIDTH - 10.0, LIST_Y + 6.0, 6.0, LIST_HEIGHT - 12.0);
		let scroll_bar = node_tree.add_component(scroll_bar_id, UIScrollBar::new()).expect("scroll bar");
		scroll_bar.set_scroll_view(Some(list_id));
		scroll_bar.set_track_color(Color::create(1.0, 1.0, 1.0, 0.05));
		scroll_bar.set_thumb_color(Color::create(1.0, 1.0, 1.0, 0.22));

		for (mode_index, definition) in MODE_DEFINITIONS.iter().enumerate() {
			let row_id = create_box_node(node_tree, content_id, &format!("EffectRow_{}", definition.key), 8.0, 8.0 + mode_index as f64 * LIST_ROW_HEIGHT, LIST_WIDTH - 16.0, LIST_ROW_HEIGHT - 6.0);
			let row_paint = node_tree[row_id].add_component(Paint::create_with_color(transparent_color()));
			row_paint.set_round_size(9.0);
			let title_id = create_label_node(node_tree, row_id, &format!("EffectRowTitle_{}", definition.key), 14.0, 4.0, LIST_WIDTH - 44.0, 20.0, 11.5, text_color(), true);
			set_label_text(node_tree, title_id, definition.title);
			let tag_id = create_label_node(node_tree, row_id, &format!("EffectRowTag_{}", definition.key), 14.0, 23.0, LIST_WIDTH - 44.0, 14.0, 9.0, text_faint_color(), false);
			set_label_text(node_tree, tag_id, definition.tag);
			let requested_mode = self.requested_mode.clone();
			let row_button = node_tree.add_component(row_id, UIButton::new()).expect("button");
			row_button.set_pressed_tint_color(Color::create(1.0, 1.0, 1.0, 0.05));
			row_button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
				requested_mode.set(Some(mode_index));
			}));
			// 선택 색은 코드가 관리하므로 버튼 틴트에서 뺀다.
			row_button.exclude_component_from_tint::<Paint>(row_id);
			row_button.exclude_node_from_tint(title_id);
			row_button.exclude_node_from_tint(tag_id);
			self.list_rows.push(ListRow { paint_node_id: row_id, title_node_id: title_id, tag_node_id: tag_id });
		}
	}

	//==============================================================================
	// 오른쪽 무대. (테두리 + 클리핑, 이펙트가 글씨 위로 뿌려진다)
	//==============================================================================
	fn build_stage(&mut self, root_id: NodeId) {
		let node_tree = self.scene_base.get_node_tree_mut();
		let frame_id = create_box_node(node_tree, root_id, "StageFrame", STAGE_X - 1.0, STAGE_Y - 1.0, STAGE_WIDTH + 2.0, STAGE_HEIGHT + 2.0);
		node_tree[frame_id].add_component(Paint::create_with_color(Color::create(0.224, 0.255, 0.329, 1.0))).set_round_size(15.0);
		let stage_id = create_box_node(node_tree, root_id, "Stage", STAGE_X, STAGE_Y, STAGE_WIDTH, STAGE_HEIGHT);
		node_tree[stage_id].add_component(Paint::create_with_color(Color::create(0.051, 0.063, 0.09, 1.0))).set_round_size(14.0);
		node_tree[stage_id].add_component(Mask::new()).set_round_size(14.0);
		self.stage_id = Some(stage_id);
	}

	//==============================================================================
	// 모드 뿌리 만들기 후 파티클 노드 붙이기. (처음에는 꺼 둔다)
	//==============================================================================
	fn add_mode(&mut self, key: &str, system_list: Vec<(Vector2, ParticleSystem)>) {
		let stage_id = self.stage_id.expect("stage");
		let node_tree = self.scene_base.get_node_tree_mut();
		let mode_root_id = node_tree.create_child(stage_id, &format!("Mode_{}", key));
		node_tree[mode_root_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[mode_root_id].set_active(false);
		let mut system_ids: Vec<NodeId> = Vec::new();
		for (position, particle_system) in system_list {
			let particle_id = node_tree.create_child(mode_root_id, "particle");
			node_tree[particle_id].set_pivot(Pivot::TOP_LEFT);
			node_tree[particle_id].set_local_position(position);
			node_tree[particle_id].add_component(particle_system);
			system_ids.push(particle_id);
		}
		self.mode_root_ids.push(mode_root_id);
		self.mode_system_ids.push(system_ids);
	}
	//==============================================================================
	// 모드 fire.
	//==============================================================================
	fn build_fire_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let center_x: f64 = STAGE_WIDTH * 0.5;
		let base_y: f64 = STAGE_HEIGHT * 0.82;
		let mut smoke = ParticleSystem::new();
		smoke.set_emitter_shape(ParticleEmitterShape::Circle);
		smoke.set_shape_radius(24.0);
		smoke.set_emission_rate(34.0);
		smoke.set_start_lifetime(1.8, 3.0);
		smoke.set_start_speed(26.0, 60.0);
		smoke.set_start_size(34.0, 70.0);
		smoke.set_gravity(10.0, -90.0);
		smoke.set_damping(0.6);
		smoke.set_start_color(Color::create(0.13, 0.13, 0.14, 0.3), Color::create(0.24, 0.23, 0.23, 0.2));
		smoke.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.25, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		smoke.set_size_over_lifetime(0.7, 2.0);
		smoke.set_max_particle_count(400);
		systems.push((Vector2::create(center_x, base_y - 50.0), smoke));
		let mut flame = ParticleSystem::new();
		flame.set_emitter_shape(ParticleEmitterShape::Cone);
		flame.set_cone_angle(0.34);
		flame.set_shape_radius(34.0);
		flame.set_emission_rate(300.0);
		flame.set_start_lifetime(0.5, 1.1);
		flame.set_start_speed(90.0, 200.0);
		flame.set_start_size(12.0, 28.0);
		flame.set_gravity(0.0, -130.0);
		flame.set_damping(1.1);
		flame.set_additive(true);
		flame.set_start_color(Color::create(1.0, 0.76, 0.3, 1.0), Color::create(1.0, 0.5, 0.12, 1.0));
		flame.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 0.85, 1.0)), (0.45, Color::create(1.0, 0.62, 0.2, 0.9)), (1.0, Color::create(0.65, 0.1, 0.02, 0.0))]);
		flame.set_size_over_lifetime(1.0, 0.25);
		flame.set_max_particle_count(1200);
		systems.push((Vector2::create(center_x, base_y), flame));
		let mut sparks = ParticleSystem::new();
		sparks.set_emitter_shape(ParticleEmitterShape::Cone);
		sparks.set_cone_angle(0.65);
		sparks.set_emission_rate(46.0);
		sparks.set_start_lifetime(0.9, 1.8);
		sparks.set_start_speed(140.0, 320.0);
		sparks.set_start_size(2.0, 4.0);
		sparks.set_gravity(0.0, 60.0);
		sparks.set_damping(0.9);
		sparks.set_additive(true);
		sparks.set_wobble(4.0, 9.0);
		sparks.set_start_color(Color::create(1.0, 0.85, 0.4, 1.0), Color::create(1.0, 0.6, 0.2, 1.0));
		sparks.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 0.4, 0.1, 0.0))]);
		sparks.set_max_particle_count(400);
		systems.push((Vector2::create(center_x, base_y - 12.0), sparks));
		systems
	}

	//==============================================================================
	// 모드 fountain.
	//==============================================================================
	fn build_fountain_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let center_x: f64 = STAGE_WIDTH * 0.5;
		let base_y: f64 = STAGE_HEIGHT * 0.9;
		let mut water = ParticleSystem::new();
		water.set_emitter_shape(ParticleEmitterShape::Cone);
		water.set_cone_angle(0.22);
		water.set_emission_rate(420.0);
		water.set_start_lifetime(1.5, 2.2);
		water.set_start_speed(420.0, 560.0);
		water.set_start_size(3.0, 7.0);
		water.set_gravity(0.0, 460.0);
		water.set_start_color(Color::create(0.55, 0.8, 1.0, 0.9), Color::create(0.8, 0.95, 1.0, 0.7));
		water.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (0.8, Color::create(1.0, 1.0, 1.0, 0.7)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		water.set_size_over_lifetime(1.0, 0.6);
		water.set_max_particle_count(2000);
		systems.push((Vector2::create(center_x, base_y), water));
		let mut mist = ParticleSystem::new();
		mist.set_emitter_shape(ParticleEmitterShape::Circle);
		mist.set_shape_radius(70.0);
		mist.set_emission_rate(18.0);
		mist.set_start_lifetime(1.4, 2.4);
		mist.set_start_speed(8.0, 26.0);
		mist.set_start_size(38.0, 74.0);
		mist.set_start_color(Color::create(0.6, 0.8, 1.0, 0.07), Color::create(0.7, 0.9, 1.0, 0.05));
		mist.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.4, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		mist.set_size_over_lifetime(0.8, 1.6);
		mist.set_max_particle_count(200);
		systems.push((Vector2::create(center_x, base_y - 210.0), mist));
		systems
	}

	//==============================================================================
	// 모드 snow.
	//==============================================================================
	fn build_snow_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut snow = ParticleSystem::new();
		snow.set_emitter_shape(ParticleEmitterShape::Edge);
		snow.set_edge_width(STAGE_WIDTH * 1.1);
		snow.set_emission_rate(170.0);
		snow.set_start_lifetime(7.0, 11.0);
		snow.set_start_speed(36.0, 80.0);
		snow.set_start_size(2.5, 6.5);
		snow.set_gravity(0.0, 10.0);
		snow.set_damping(0.12);
		snow.set_wobble(16.0, 1.4);
		snow.set_start_color(Color::create(1.0, 1.0, 1.0, 0.95), Color::create(0.8, 0.88, 1.0, 0.55));
		snow.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.06, Color::create(1.0, 1.0, 1.0, 1.0)), (0.92, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		snow.set_max_particle_count(2200);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, -16.0), snow));
		systems
	}

	//==============================================================================
	// 모드 rain.
	//==============================================================================
	fn build_rain_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut rain = ParticleSystem::new();
		rain.set_emitter_shape(ParticleEmitterShape::Edge);
		rain.set_edge_width(STAGE_WIDTH * 1.2);
		rain.set_emission_rate(520.0);
		rain.set_start_lifetime(1.1, 1.4);
		rain.set_start_speed(500.0, 620.0);
		rain.set_start_size(1.4, 2.2);
		rain.set_render_shape(ParticleRenderShape::Streak);
		rain.set_streak_scale(0.03);
		rain.set_start_color(Color::create(0.62, 0.74, 0.95, 0.55), Color::create(0.75, 0.85, 1.0, 0.35));
		rain.set_max_particle_count(2400);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, -12.0), rain));
		let mut splash = ParticleSystem::new();
		splash.set_emitter_shape(ParticleEmitterShape::Edge);
		splash.set_edge_width(STAGE_WIDTH * 1.05);
		splash.set_emission_rate(240.0);
		splash.set_start_lifetime(0.25, 0.45);
		splash.set_start_speed(40.0, 130.0);
		splash.set_start_size(1.2, 2.4);
		splash.set_gravity(0.0, 700.0);
		splash.set_start_color(Color::create(0.7, 0.8, 1.0, 0.5), Color::create(0.85, 0.92, 1.0, 0.3));
		splash.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		splash.set_max_particle_count(600);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT - 8.0), splash));
		systems
	}

	//==============================================================================
	// 모드 explosion.
	//==============================================================================
	fn build_explosion_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut smoke = ParticleSystem::new();
		smoke.set_emission_rate(0.0);
		smoke.set_emitter_shape(ParticleEmitterShape::Circle);
		smoke.set_shape_radius(26.0);
		smoke.set_start_lifetime(1.3, 2.4);
		smoke.set_start_speed(30.0, 130.0);
		smoke.set_start_size(24.0, 54.0);
		smoke.set_gravity(0.0, -50.0);
		smoke.set_damping(1.4);
		smoke.set_start_color(Color::create(0.18, 0.17, 0.16, 0.5), Color::create(0.3, 0.28, 0.26, 0.35));
		smoke.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		smoke.set_size_over_lifetime(0.8, 2.3);
		smoke.set_max_particle_count(600);
		systems.push((Vector2::create(0.0, 0.0), smoke));
		let mut flash = ParticleSystem::new();
		flash.set_emission_rate(0.0);
		flash.set_start_lifetime(0.16, 0.24);
		flash.set_start_speed(0.0, 10.0);
		flash.set_start_size(80.0, 120.0);
		flash.set_additive(true);
		flash.set_start_color(Color::create(1.0, 0.9, 0.6, 1.0), Color::create(1.0, 0.9, 0.6, 1.0));
		flash.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 0.6, 0.2, 0.0))]);
		flash.set_size_over_lifetime(0.6, 1.7);
		systems.push((Vector2::create(0.0, 0.0), flash));
		let mut sparks = ParticleSystem::new();
		sparks.set_emission_rate(0.0);
		sparks.set_emitter_shape(ParticleEmitterShape::Circle);
		sparks.set_shape_radius(10.0);
		sparks.set_start_lifetime(0.5, 1.3);
		sparks.set_start_speed(260.0, 680.0);
		sparks.set_start_size(2.5, 4.5);
		sparks.set_gravity(0.0, 360.0);
		sparks.set_damping(2.2);
		sparks.set_additive(true);
		sparks.set_render_shape(ParticleRenderShape::Streak);
		sparks.set_streak_scale(0.045);
		sparks.set_start_color(Color::create(1.0, 0.8, 0.35, 1.0), Color::create(1.0, 0.5, 0.15, 1.0));
		sparks.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 0.3, 0.05, 0.0))]);
		sparks.set_max_particle_count(1600);
		systems.push((Vector2::create(0.0, 0.0), sparks));
		systems
	}

	//==============================================================================
	// 모드 sparkle.
	//==============================================================================
	fn build_sparkle_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut sparkle = ParticleSystem::new();
		sparkle.set_emitter_shape(ParticleEmitterShape::Box);
		sparkle.set_box_size(STAGE_WIDTH * 0.94, STAGE_HEIGHT * 0.86);
		sparkle.set_emission_rate(160.0);
		sparkle.set_start_lifetime(1.2, 2.6);
		sparkle.set_start_speed(0.0, 8.0);
		sparkle.set_start_size(2.0, 7.0);
		sparkle.set_additive(true);
		sparkle.set_start_color(Color::create(0.75, 0.85, 1.0, 1.0), Color::create(1.0, 0.9, 0.7, 1.0));
		sparkle.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.3, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		sparkle.set_size_over_lifetime(0.4, 1.3);
		sparkle.set_max_particle_count(1200);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT * 0.5), sparkle));
		systems
	}

	//==============================================================================
	// 모드 embers.
	//==============================================================================
	fn build_embers_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut embers = ParticleSystem::new();
		embers.set_emitter_shape(ParticleEmitterShape::Edge);
		embers.set_edge_width(STAGE_WIDTH * 0.96);
		embers.set_emission_rate(130.0);
		embers.set_start_lifetime(2.8, 4.6);
		embers.set_start_speed(0.0, 20.0);
		embers.set_start_size(2.0, 5.0);
		embers.set_gravity(0.0, -85.0);
		embers.set_damping(0.25);
		embers.set_wobble(18.0, 1.8);
		embers.set_additive(true);
		embers.set_start_color(Color::create(1.0, 0.6, 0.2, 1.0), Color::create(1.0, 0.8, 0.4, 0.8));
		embers.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.12, Color::create(1.0, 1.0, 1.0, 1.0)), (0.7, Color::create(1.0, 0.8, 0.6, 0.8)), (1.0, Color::create(0.6, 0.2, 0.05, 0.0))]);
		embers.set_size_over_lifetime(1.0, 0.4);
		embers.set_max_particle_count(1000);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT - 4.0), embers));
		systems
	}

	//==============================================================================
	// 모드 bubbles.
	//==============================================================================
	fn build_bubbles_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut bubbles = ParticleSystem::new();
		bubbles.set_emitter_shape(ParticleEmitterShape::Edge);
		bubbles.set_edge_width(STAGE_WIDTH * 0.9);
		bubbles.set_emission_rate(75.0);
		bubbles.set_start_lifetime(2.8, 4.8);
		bubbles.set_start_speed(0.0, 14.0);
		bubbles.set_start_size(4.0, 15.0);
		bubbles.set_gravity(0.0, -75.0);
		bubbles.set_damping(0.2);
		bubbles.set_wobble(22.0, 2.4);
		bubbles.set_start_color(Color::create(0.6, 0.85, 1.0, 0.5), Color::create(0.8, 0.95, 1.0, 0.3));
		bubbles.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.1, Color::create(1.0, 1.0, 1.0, 1.0)), (0.85, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		bubbles.set_size_over_lifetime(0.7, 1.15);
		bubbles.set_max_particle_count(900);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT - 6.0), bubbles));
		systems
	}

	//==============================================================================
	// 모드 vortex.
	//==============================================================================
	fn build_vortex_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let center_x: f64 = STAGE_WIDTH * 0.5;
		let center_y: f64 = STAGE_HEIGHT * 0.5;
		let mut vortex = ParticleSystem::new();
		vortex.set_emitter_shape(ParticleEmitterShape::Circle);
		vortex.set_shape_radius(280.0);
		vortex.set_emission_rate(360.0);
		vortex.set_start_lifetime(1.8, 3.4);
		vortex.set_start_speed(10.0, 60.0);
		vortex.set_start_size(2.0, 5.0);
		vortex.set_additive(true);
		vortex.set_attractor(0.0, 0.0, 190.0, 260.0);
		vortex.set_damping(0.35);
		vortex.set_start_color(Color::create(0.5, 0.7, 1.0, 1.0), Color::create(0.8, 0.6, 1.0, 1.0));
		vortex.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.15, Color::create(1.0, 1.0, 1.0, 1.0)), (0.85, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		vortex.set_size_over_lifetime(1.0, 0.5);
		vortex.set_max_particle_count(2000);
		systems.push((Vector2::create(center_x, center_y), vortex));
		let mut core = ParticleSystem::new();
		core.set_emission_rate(30.0);
		core.set_emitter_shape(ParticleEmitterShape::Circle);
		core.set_shape_radius(10.0);
		core.set_start_lifetime(0.5, 0.9);
		core.set_start_speed(0.0, 12.0);
		core.set_start_size(16.0, 30.0);
		core.set_additive(true);
		core.set_start_color(Color::create(0.7, 0.8, 1.0, 0.5), Color::create(0.9, 0.85, 1.0, 0.35));
		core.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.4, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		core.set_max_particle_count(120);
		systems.push((Vector2::create(center_x, center_y), core));
		systems
	}

	//==============================================================================
	// 모드 meteor.
	//==============================================================================
	fn build_meteor_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut meteor = ParticleSystem::new();
		meteor.set_emitter_shape(ParticleEmitterShape::Edge);
		meteor.set_edge_width(STAGE_WIDTH * 1.5);
		meteor.set_emission_rate(7.0);
		meteor.set_start_lifetime(1.0, 1.5);
		meteor.set_start_speed(60.0, 130.0);
		meteor.set_start_size(2.0, 3.6);
		meteor.set_gravity(420.0, 340.0);
		meteor.set_render_shape(ParticleRenderShape::Streak);
		meteor.set_streak_scale(0.09);
		meteor.set_additive(true);
		meteor.set_start_color(Color::create(1.0, 0.92, 0.72, 1.0), Color::create(0.8, 0.88, 1.0, 1.0));
		meteor.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.12, Color::create(1.0, 1.0, 1.0, 1.0)), (0.85, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		meteor.set_max_particle_count(60);
		systems.push((Vector2::create(STAGE_WIDTH * 0.18, -20.0), meteor));
		let mut dust = ParticleSystem::new();
		dust.set_emitter_shape(ParticleEmitterShape::Box);
		dust.set_box_size(STAGE_WIDTH * 0.94, STAGE_HEIGHT * 0.86);
		dust.set_emission_rate(16.0);
		dust.set_start_lifetime(2.4, 4.2);
		dust.set_start_speed(0.0, 6.0);
		dust.set_start_size(1.0, 2.2);
		dust.set_additive(true);
		dust.set_start_color(Color::create(0.9, 0.94, 1.0, 0.8), Color::create(1.0, 0.98, 0.9, 0.6));
		dust.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.4, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		dust.set_max_particle_count(120);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT * 0.5), dust));
		systems
	}

	//==============================================================================
	// 모드 lightning.
	//==============================================================================
	fn build_lightning_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut bolt = ParticleSystem::new();
		bolt.set_emitter_shape(ParticleEmitterShape::Point);
		bolt.set_emission_rate(0.0);
		bolt.set_start_lifetime(0.2, 0.45);
		bolt.set_start_speed(0.0, 8.0);
		bolt.set_start_size(2.0, 3.6);
		bolt.set_additive(true);
		bolt.set_start_color(Color::create(1.0, 1.0, 1.0, 1.0), Color::create(0.8, 0.85, 1.0, 1.0));
		bolt.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(0.6, 0.7, 1.0, 0.0))]);
		bolt.set_max_particle_count(700);
		systems.push((Vector2::create(0.0, 0.0), bolt));
		let mut flash = ParticleSystem::new();
		flash.set_emitter_shape(ParticleEmitterShape::Point);
		flash.set_emission_rate(0.0);
		flash.set_start_lifetime(0.18, 0.28);
		flash.set_start_speed(0.0, 0.0);
		flash.set_start_size(420.0, 520.0);
		flash.set_additive(true);
		flash.set_start_color(Color::create(0.8, 0.85, 1.0, 0.1), Color::create(0.9, 0.92, 1.0, 0.08));
		flash.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		flash.set_max_particle_count(8);
		systems.push((Vector2::create(0.0, 0.0), flash));
		let mut sparks = ParticleSystem::new();
		sparks.set_emitter_shape(ParticleEmitterShape::Circle);
		sparks.set_shape_radius(4.0);
		sparks.set_emission_rate(0.0);
		sparks.set_start_lifetime(0.4, 0.9);
		sparks.set_start_speed(120.0, 320.0);
		sparks.set_start_size(1.6, 3.0);
		sparks.set_gravity(0.0, 340.0);
		sparks.set_additive(true);
		sparks.set_start_color(Color::create(1.0, 1.0, 0.85, 1.0), Color::create(0.8, 0.9, 1.0, 1.0));
		sparks.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		sparks.set_max_particle_count(200);
		systems.push((Vector2::create(0.0, 0.0), sparks));
		systems
	}

	//==============================================================================
	// 모드 swarm.
	//==============================================================================
	fn build_swarm_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let mut swarm = ParticleSystem::new();
		swarm.set_emitter_shape(ParticleEmitterShape::Circle);
		swarm.set_shape_radius(180.0);
		swarm.set_emission_rate(220.0);
		swarm.set_start_lifetime(1.8, 3.2);
		swarm.set_start_speed(20.0, 60.0);
		swarm.set_start_size(1.6, 3.2);
		swarm.set_damping(0.5);
		swarm.set_wobble(6.0, 6.0);
		swarm.set_attractor(0.0, 0.0, 620.0, 140.0);
		swarm.set_start_color(Color::create(1.0, 0.85, 0.4, 1.0), Color::create(0.85, 0.65, 0.25, 1.0));
		swarm.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.12, Color::create(1.0, 1.0, 1.0, 1.0)), (0.9, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		swarm.set_max_particle_count(800);
		systems.push((Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT * 0.5), swarm));
		systems
	}

	//==============================================================================
	// 모드 volcano.
	//==============================================================================
	fn build_volcano_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let crater_x: f64 = STAGE_WIDTH * 0.5;
		let crater_y: f64 = STAGE_HEIGHT * 0.8;
		let mut smoke = ParticleSystem::new();
		smoke.set_emitter_shape(ParticleEmitterShape::Circle);
		smoke.set_shape_radius(20.0);
		smoke.set_emission_rate(46.0);
		smoke.set_start_lifetime(2.2, 3.8);
		smoke.set_start_speed(40.0, 90.0);
		smoke.set_start_size(22.0, 46.0);
		smoke.set_gravity(8.0, -120.0);
		smoke.set_damping(0.5);
		smoke.set_start_color(Color::create(0.32, 0.27, 0.24, 0.34), Color::create(0.5, 0.42, 0.36, 0.22));
		smoke.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.2, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		smoke.set_size_over_lifetime(0.8, 2.0);
		smoke.set_max_particle_count(300);
		systems.push((Vector2::create(crater_x, crater_y - 10.0), smoke));
		let mut lava = ParticleSystem::new();
		lava.set_emitter_shape(ParticleEmitterShape::Cone);
		lava.set_cone_angle(0.5);
		lava.set_shape_radius(18.0);
		lava.set_emission_rate(0.0);
		lava.set_start_lifetime(1.3, 2.1);
		lava.set_start_speed(320.0, 560.0);
		lava.set_start_size(4.0, 9.0);
		lava.set_gravity(0.0, 640.0);
		lava.set_start_color(Color::create(1.0, 0.55, 0.15, 1.0), Color::create(1.0, 0.3, 0.05, 1.0));
		lava.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 0.9, 1.0)), (0.6, Color::create(1.0, 0.6, 0.2, 1.0)), (1.0, Color::create(0.5, 0.1, 0.02, 0.0))]);
		lava.set_max_particle_count(400);
		systems.push((Vector2::create(crater_x, crater_y), lava));
		let mut glow = ParticleSystem::new();
		glow.set_emitter_shape(ParticleEmitterShape::Circle);
		glow.set_shape_radius(26.0);
		glow.set_emission_rate(90.0);
		glow.set_start_lifetime(0.5, 1.1);
		glow.set_start_speed(30.0, 90.0);
		glow.set_start_size(2.0, 4.5);
		glow.set_gravity(0.0, -40.0);
		glow.set_additive(true);
		glow.set_start_color(Color::create(1.0, 0.7, 0.25, 1.0), Color::create(1.0, 0.45, 0.1, 1.0));
		glow.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		glow.set_max_particle_count(200);
		systems.push((Vector2::create(crater_x, crater_y - 4.0), glow));
		systems
	}

	//==============================================================================
	// 모드 warp.
	//==============================================================================
	fn build_warp_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		let center_x: f64 = STAGE_WIDTH * 0.5;
		let center_y: f64 = STAGE_HEIGHT * 0.5;
		let mut warp = ParticleSystem::new();
		warp.set_emitter_shape(ParticleEmitterShape::Circle);
		warp.set_shape_radius(26.0);
		warp.set_emission_rate(460.0);
		warp.set_start_lifetime(0.5, 0.9);
		warp.set_start_speed(480.0, 880.0);
		warp.set_start_size(1.5, 3.0);
		warp.set_render_shape(ParticleRenderShape::Streak);
		warp.set_streak_scale(0.1);
		warp.set_additive(true);
		warp.set_start_color(Color::create(0.7, 0.85, 1.0, 1.0), Color::create(0.92, 0.96, 1.0, 1.0));
		warp.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.2, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		warp.set_max_particle_count(1400);
		systems.push((Vector2::create(center_x, center_y), warp));
		let mut core_glow = ParticleSystem::new();
		core_glow.set_emitter_shape(ParticleEmitterShape::Circle);
		core_glow.set_shape_radius(8.0);
		core_glow.set_emission_rate(18.0);
		core_glow.set_start_lifetime(0.5, 1.0);
		core_glow.set_start_speed(0.0, 8.0);
		core_glow.set_start_size(26.0, 46.0);
		core_glow.set_additive(true);
		core_glow.set_start_color(Color::create(0.8, 0.9, 1.0, 0.3), Color::create(0.9, 0.95, 1.0, 0.2));
		core_glow.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.4, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		core_glow.set_max_particle_count(60);
		systems.push((Vector2::create(center_x, center_y), core_glow));
		systems
	}

	//==============================================================================
	// 모드 색종이. (자동 분출, 시스템마다 회전 방향을 갈라 자연스럽게)
	//==============================================================================
	fn build_confetti_mode() -> Vec<(Vector2, ParticleSystem)> {
		let setup_list = [
			([Color::create(1.0, 0.45, 0.55, 1.0), Color::create(1.0, 0.72, 0.35, 1.0)], (2.2, 3.6)),
			([Color::create(0.4, 0.75, 1.0, 1.0), Color::create(0.45, 1.0, 0.72, 1.0)], (-3.6, -2.2)),
			([Color::create(0.78, 0.58, 1.0, 1.0), Color::create(1.0, 0.9, 0.45, 1.0)], (2.6, 4.0)),
		];
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		for (colors, spin) in setup_list {
			let mut confetti = ParticleSystem::new();
			confetti.set_emission_rate(0.0);
			confetti.set_emitter_shape(ParticleEmitterShape::Circle);
			confetti.set_shape_radius(12.0);
			confetti.set_start_lifetime(1.8, 3.0);
			confetti.set_start_speed(160.0, 420.0);
			confetti.set_start_size(4.5, 8.0);
			confetti.set_gravity(0.0, 460.0);
			confetti.set_damping(1.4);
			confetti.set_render_shape(ParticleRenderShape::Rect);
			confetti.set_start_rotation(0.0, std::f64::consts::PI * 2.0);
			confetti.set_angular_velocity(spin.0, spin.1);
			confetti.set_wobble(10.0, 3.0);
			confetti.set_start_color(colors[0], colors[1]);
			confetti.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (0.85, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
			confetti.set_max_particle_count(1200);
			systems.push((Vector2::create(0.0, 0.0), confetti));
		}
		systems
	}

	//==============================================================================
	// 모드 불꽃놀이. (자동 분출)
	//==============================================================================
	fn build_firework_mode() -> Vec<(Vector2, ParticleSystem)> {
		let palette_list = [
			[Color::create(1.0, 0.5, 0.6, 1.0), Color::create(1.0, 0.8, 0.5, 1.0)],
			[Color::create(0.5, 0.8, 1.0, 1.0), Color::create(0.7, 1.0, 0.9, 1.0)],
			[Color::create(0.8, 0.6, 1.0, 1.0), Color::create(1.0, 0.85, 0.5, 1.0)],
			[Color::create(0.55, 1.0, 0.6, 1.0), Color::create(1.0, 1.0, 0.7, 1.0)],
		];
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		for palette in palette_list {
			let mut burst = ParticleSystem::new();
			burst.set_emission_rate(0.0);
			burst.set_emitter_shape(ParticleEmitterShape::Circle);
			burst.set_shape_radius(4.0);
			burst.set_start_lifetime(1.1, 2.0);
			burst.set_start_speed(130.0, 380.0);
			burst.set_start_size(2.5, 4.5);
			burst.set_gravity(0.0, 150.0);
			burst.set_damping(1.1);
			burst.set_additive(true);
			burst.set_start_color(palette[0], palette[1]);
			burst.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (0.7, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
			burst.set_max_particle_count(1400);
			systems.push((Vector2::create(0.0, 0.0), burst));
		}
		systems
	}

	//==============================================================================
	// 모드 마법 궤적. (리사주 곡선 자동 주행, 같은 노드에 TrailRenderer 와 ParticleSystem)
	//==============================================================================
	fn build_trail_mode() -> Vec<(Vector2, ParticleSystem)> {
		let mut sparkles = ParticleSystem::new();
		sparkles.set_emitter_shape(ParticleEmitterShape::Circle);
		sparkles.set_shape_radius(7.0);
		sparkles.set_emission_rate(240.0);
		sparkles.set_start_lifetime(0.4, 1.1);
		sparkles.set_start_speed(8.0, 70.0);
		sparkles.set_start_size(1.6, 4.5);
		sparkles.set_gravity(0.0, 80.0);
		sparkles.set_additive(true);
		sparkles.set_start_color(Color::create(0.7, 0.85, 1.0, 1.0), Color::create(0.85, 0.7, 1.0, 1.0));
		sparkles.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		sparkles.set_max_particle_count(600);
		vec![(Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT * 0.5), sparkles)]
	}

	//==============================================================================
	// 모드 은하. (이미터 노드가 공전하며 월드 공간에 궤적을 남긴다)
	//==============================================================================
	fn build_galaxy_mode() -> Vec<(Vector2, ParticleSystem)> {
		let center_x: f64 = STAGE_WIDTH * 0.5;
		let center_y: f64 = STAGE_HEIGHT * 0.5;
		let arm_color_table = [
			[Color::create(0.65, 0.8, 1.0, 1.0), Color::create(0.9, 0.95, 1.0, 1.0)],
			[Color::create(0.85, 0.7, 1.0, 1.0), Color::create(1.0, 0.9, 0.8, 1.0)],
			[Color::create(1.0, 0.9, 0.65, 1.0), Color::create(0.8, 0.85, 1.0, 1.0)],
		];
		let mut systems: Vec<(Vector2, ParticleSystem)> = Vec::new();
		for arm_colors in arm_color_table {
			let mut arm = ParticleSystem::new();
			arm.set_emitter_shape(ParticleEmitterShape::Circle);
			arm.set_shape_radius(7.0);
			arm.set_emission_rate(240.0);
			arm.set_start_lifetime(2.2, 3.6);
			arm.set_start_speed(3.0, 16.0);
			arm.set_start_size(1.4, 3.0);
			arm.set_additive(true);
			arm.set_world_space(true);
			arm.set_start_color(arm_colors[0], arm_colors[1]);
			arm.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.1, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
			arm.set_size_over_lifetime(1.0, 0.4);
			arm.set_max_particle_count(900);
			systems.push((Vector2::create(center_x, center_y), arm));
		}
		let mut core = ParticleSystem::new();
		core.set_emitter_shape(ParticleEmitterShape::Circle);
		core.set_shape_radius(10.0);
		core.set_emission_rate(22.0);
		core.set_start_lifetime(0.6, 1.2);
		core.set_start_speed(0.0, 10.0);
		core.set_start_size(24.0, 44.0);
		core.set_additive(true);
		core.set_start_color(Color::create(1.0, 0.95, 0.8, 0.35), Color::create(0.9, 0.85, 1.0, 0.25));
		core.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.4, Color::create(1.0, 1.0, 1.0, 1.0)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		core.set_max_particle_count(80);
		systems.push((Vector2::create(center_x, center_y), core));
		systems
	}

	//==============================================================================
	// 무대 위 덮개. (모드 뒤에 붙여 이펙트 위에 그린다, 사용 코드 패널 + 통계 패널)
	//==============================================================================
	fn build_stage_overlay(&mut self) {
		let stage_id = self.stage_id.expect("stage");
		let node_tree = self.scene_base.get_node_tree_mut();

		// 사용 코드 패널. (제목 + 코드 + 복사 단추)
		let usage_panel_id = create_box_node(node_tree, stage_id, "UsagePanel", 16.0, 16.0, 420.0, 120.0);
		node_tree[usage_panel_id].add_component(Paint::create_with_color(Color::create(0.0, 0.0, 0.0, 0.5))).set_round_size(10.0);
		self.stage_title_id = Some(create_label_node(node_tree, usage_panel_id, "StageTitle", 12.0, 9.0, 280.0, 22.0, 15.0, text_color(), true));
		let usage_id = create_label_node(node_tree, usage_panel_id, "StageUsage", 12.0, 40.0, 396.0, 78.0, 10.5, text_dim_color(), false);
		if let Some(usage_label) = node_tree[usage_id].get_component_mut::<UILabel>() {
			usage_label.set_text_baseline(TextBaseline::Top);
		}
		self.stage_usage_id = Some(usage_id);

		let copy_button_id = create_box_node(node_tree, usage_panel_id, "UsageCopyButton", 352.0, 8.0, 52.0, 19.0);
		node_tree[copy_button_id].add_component(Paint::create_with_color(Color::create(1.0, 1.0, 1.0, 0.09))).set_round_size(6.0);
		let copy_label_id = create_box_node(node_tree, copy_button_id, "UsageCopyLabel", 0.0, 0.0, 52.0, 19.0);
		{
			let copy_label = node_tree.add_component(copy_label_id, UILabel::create_with_text("COPY", 8.5)).expect("label");
			copy_label.set_bold(true);
			copy_label.set_text_color(text_dim_color());
			copy_label.set_text_align(TextAlign::Center);
		}
		let is_copy_requested = self.is_copy_requested.clone();
		let copy_button = node_tree.add_component(copy_button_id, UIButton::new()).expect("button");
		copy_button.set_pressed_tint_color(Color::create(1.0, 1.0, 1.0, 0.14));
		copy_button.set_clicked_event(Box::new(move |_button: &mut UIButton, _context: &mut ComponentContext| {
			is_copy_requested.set(true);
		}));
		copy_button.exclude_node_from_tint(copy_label_id);
		self.usage_panel_id = Some(usage_panel_id);
		self.copy_button_id = Some(copy_button_id);
		self.copy_label_id = Some(copy_label_id);

		// 렌더 통계 패널.
		let stats_panel_id = create_box_node(node_tree, stage_id, "StatsPanel", STAGE_WIDTH - 204.0, STAGE_HEIGHT - 106.0, 188.0, 90.0);
		node_tree[stats_panel_id].add_component(Paint::create_with_color(Color::create(0.0, 0.0, 0.0, 0.5))).set_round_size(10.0);
		let mut stat_ids: Vec<NodeId> = Vec::new();
		for (stat_index, stat_name) in ["FpsReadout", "ParticleCount", "VertexCount", "TriangleCount"].iter().enumerate() {
			let stat_id = create_label_node(node_tree, stats_panel_id, stat_name, 12.0, 8.0 + stat_index as f64 * 19.0, 164.0, 16.0, 10.5, text_dim_color(), false);
			if let Some(stat_label) = node_tree[stat_id].get_component_mut::<UILabel>() {
				stat_label.set_text_align(TextAlign::Right);
			}
			stat_ids.push(stat_id);
		}
		self.fps_label_id = Some(stat_ids[0]);
		self.count_label_id = Some(stat_ids[1]);
		self.vertex_label_id = Some(stat_ids[2]);
		self.triangle_label_id = Some(stat_ids[3]);
	}

	//==============================================================================
	// 번개 한 번 내리치기. (지그재그 경로를 만들어 좌표 지정 방출로 그린다)
	//==============================================================================
	fn strike_lightning(&mut self) {
		let system_ids = self.mode_system_ids[MODE_LIGHTNING].clone();
		let (bolt_id, flash_id, sparks_id) = (system_ids[0], system_ids[1], system_ids[2]);
		let random = &mut self.random;
		let node_tree = self.scene_base.get_node_tree_mut();
		let mut path_x = random.next_range(STAGE_WIDTH * 0.2, STAGE_WIDTH * 0.8);
		let mut path_y = 0.0;
		let end_y = STAGE_HEIGHT * random.next_range(0.55, 0.8);
		let segment_count = 12;
		let step_y = end_y / segment_count as f64;
		for _ in 0..segment_count {
			let next_x = path_x + random.next_range(-52.0, 52.0);
			let next_y = path_y + step_y * random.next_range(0.7, 1.3);
			let stamp_count = 4;
			for stamp_index in 0..stamp_count {
				let stamp_t = stamp_index as f64 / stamp_count as f64;
				let stamp_x = path_x + (next_x - path_x) * stamp_t;
				let stamp_y = path_y + (next_y - path_y) * stamp_t;
				ParticleSystem::emit_in_tree(node_tree, bolt_id, 1, Some(Vector2::create(stamp_x, stamp_y)));
			}
			// 가지 하나를 낮은 확률로 뻗는다.
			if random.next_range(0.0, 1.0) < 0.3 {
				let branch_angle = random.next_range(0.6, 2.5);
				let branch_length = random.next_range(30.0, 90.0);
				let branch_steps = 4;
				for branch_index in 1..=branch_steps {
					let branch_t = branch_index as f64 / branch_steps as f64;
					let branch_sign = if random.next_range(0.0, 1.0) < 0.5 { -1.0 } else { 1.0 };
					let branch_x = path_x + branch_angle.cos() * branch_length * branch_t * branch_sign;
					let branch_y = path_y + branch_angle.sin() * branch_length * branch_t;
					ParticleSystem::emit_in_tree(node_tree, bolt_id, 1, Some(Vector2::create(branch_x, branch_y)));
				}
			}
			path_x = next_x;
			path_y = next_y;
		}
		ParticleSystem::emit_in_tree(node_tree, flash_id, 1, Some(Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT * 0.35)));
		ParticleSystem::emit_in_tree(node_tree, sparks_id, 20, Some(Vector2::create(path_x, path_y)));
	}

	//==============================================================================
	// 모드 고르기. (이전 모드의 파티클을 지우고 새 모드를 재생)
	//==============================================================================
	fn select_mode(&mut self, mode_index: usize) {
		self.mode_index = mode_index;
		self.burst_accumulator = 0.0;
		let node_tree = self.scene_base.get_node_tree_mut();
		for (index, mode_root_id) in self.mode_root_ids.iter().enumerate() {
			let is_active = index == mode_index;
			node_tree[*mode_root_id].set_active(is_active);
			for particle_id in self.mode_system_ids[index].iter() {
				if let Some(particle_system) = node_tree[*particle_id].get_component_mut::<ParticleSystem>() {
					particle_system.stop(true);
					if is_active {
						particle_system.play();
					}
				}
			}
		}
		if let Some(trail_renderer) = self.trail_node_id.and_then(|trail_node_id| node_tree[trail_node_id].get_component_mut::<TrailRenderer>()) {
			trail_renderer.clear();
		}
		if mode_index == MODE_LIGHTNING {
			self.strike_lightning();
		}
		let node_tree = self.scene_base.get_node_tree_mut();
		for (index, list_row) in self.list_rows.iter().enumerate() {
			let is_active = index == mode_index;
			if let Some(row_paint) = node_tree[list_row.paint_node_id].get_component_mut::<Paint>() {
				row_paint.set_color(if is_active { accent_color() } else { transparent_color() });
			}
			set_label_color(node_tree, list_row.title_node_id, if is_active { on_accent_color() } else { text_color() });
			set_label_color(node_tree, list_row.tag_node_id, if is_active { Color::create(0.165, 0.126, 0.086, 0.72) } else { text_faint_color() });
		}
		let definition = &MODE_DEFINITIONS[mode_index];
		set_label_text(node_tree, self.stage_title_id.expect("title"), definition.title);
		set_label_text(node_tree, self.stage_usage_id.expect("usage"), &definition.usage.join("\n"));
		// 사용 코드 패널을 코드 줄 수와 가장 긴 줄 길이에 맞춘다.
		let longest_line_length = definition.usage.iter().map(|usage_line| usage_line.chars().count()).max().unwrap_or(0) as f64;
		let usage_line_height = 10.5 * 1.25;
		let code_width = longest_line_length * 5.9 + 24.0 + 66.0;
		let title_width = definition.title.chars().count() as f64 * 9.5 + 100.0;
		let panel_width = (STAGE_WIDTH - 32.0).min(code_width.max(title_width));
		let panel_height = 40.0 + definition.usage.len() as f64 * usage_line_height + 12.0;
		node_tree[self.usage_panel_id.expect("panel")].set_content_size(Vector2::create(panel_width, panel_height));
		node_tree[self.copy_button_id.expect("copy")].set_local_position(Vector2::create(panel_width - 60.0, 8.0));
		node_tree[self.stage_usage_id.expect("usage")].set_content_size(Vector2::create(panel_width - 24.0, panel_height - 48.0));
		set_label_text(node_tree, self.copy_label_id.expect("copy label"), "COPY");
		set_label_color(node_tree, self.copy_label_id.expect("copy label"), text_dim_color());
		self.copy_reset_seconds = 0.0;
	}

	//==============================================================================
	// 눈에 띄는 입자 수 합.
	//==============================================================================
	fn count_particles(&self) -> usize {
		let node_tree = self.scene_base.get_node_tree();
		let mut total_count = 0;
		for particle_id in self.mode_system_ids[self.mode_index].iter() {
			if let Some(particle_system) = node_tree[*particle_id].get_component::<ParticleSystem>() {
				total_count += particle_system.get_particle_count();
			}
		}
		total_count
	}

	//==============================================================================
	// 자동 연출 진행. (분출 주기, 궤적, 공전, 끌개 움직임)
	//==============================================================================
	fn run_automation(&mut self, time_delta: f64) {
		self.elapsed_seconds += time_delta;
		let mode_index = self.mode_index;
		let system_ids = self.mode_system_ids[mode_index].clone();
		match mode_index {
			MODE_CONFETTI => {
				self.burst_accumulator += time_delta;
				if self.burst_accumulator >= 0.9 {
					self.burst_accumulator -= 0.9;
					let burst_x = self.random.next_range(STAGE_WIDTH * 0.15, STAGE_WIDTH * 0.85);
					let burst_y = self.random.next_range(STAGE_HEIGHT * 0.12, STAGE_HEIGHT * 0.4);
					let node_tree = self.scene_base.get_node_tree_mut();
					for particle_id in system_ids {
						ParticleSystem::emit_in_tree(node_tree, particle_id, 40, Some(Vector2::create(burst_x, burst_y)));
					}
				}
			}
			MODE_EXPLOSION => {
				self.burst_accumulator += time_delta;
				if self.burst_accumulator >= 1.2 {
					self.burst_accumulator -= 1.2;
					let burst_x = self.random.next_range(STAGE_WIDTH * 0.2, STAGE_WIDTH * 0.8);
					let burst_y = self.random.next_range(STAGE_HEIGHT * 0.25, STAGE_HEIGHT * 0.75);
					let node_tree = self.scene_base.get_node_tree_mut();
					ParticleSystem::emit_in_tree(node_tree, system_ids[0], 20, Some(Vector2::create(burst_x, burst_y)));
					ParticleSystem::emit_in_tree(node_tree, system_ids[1], 4, Some(Vector2::create(burst_x, burst_y)));
					ParticleSystem::emit_in_tree(node_tree, system_ids[2], 110, Some(Vector2::create(burst_x, burst_y)));
				}
			}
			MODE_FIREWORK => {
				self.burst_accumulator += time_delta;
				if self.burst_accumulator >= 0.65 {
					self.burst_accumulator -= 0.65;
					let burst_id = system_ids[self.burst_cursor % system_ids.len()];
					self.burst_cursor += 1;
					let burst_x = self.random.next_range(STAGE_WIDTH * 0.12, STAGE_WIDTH * 0.88);
					let burst_y = self.random.next_range(STAGE_HEIGHT * 0.15, STAGE_HEIGHT * 0.55);
					ParticleSystem::emit_in_tree(self.scene_base.get_node_tree_mut(), burst_id, 130, Some(Vector2::create(burst_x, burst_y)));
				}
			}
			MODE_TRAIL => {
				// 리사주 곡선 주행.
				let path_time = self.elapsed_seconds * 0.8;
				let path_x = STAGE_WIDTH * 0.5 + STAGE_WIDTH * 0.38 * (path_time * 1.0).sin();
				let path_y = STAGE_HEIGHT * 0.5 + STAGE_HEIGHT * 0.34 * (path_time * 1.7 + 1.3).sin();
				if let Some(trail_node_id) = self.trail_node_id {
					self.scene_base.get_node_tree_mut()[trail_node_id].set_local_position(Vector2::create(path_x, path_y));
				}
			}
			MODE_LIGHTNING => {
				self.burst_accumulator += time_delta;
				if self.burst_accumulator >= 1.4 {
					self.burst_accumulator -= 1.4;
					self.strike_lightning();
				}
			}
			MODE_VOLCANO => {
				self.burst_accumulator += time_delta;
				if self.burst_accumulator >= 1.7 {
					self.burst_accumulator -= 1.7;
					ParticleSystem::emit_in_tree(self.scene_base.get_node_tree_mut(), system_ids[1], 26, None);
				}
			}
			MODE_SWARM => {
				// 도망 다니는 목표를 끌개로 쫓는다. (이미터 중심 기준 리사주)
				let chase_time = self.elapsed_seconds;
				let target_x = 320.0 * (chase_time * 0.9).sin();
				let target_y = 200.0 * (chase_time * 1.5 + 0.7).sin();
				if let Some(swarm) = self.scene_base.get_node_tree_mut()[system_ids[0]].get_component_mut::<ParticleSystem>() {
					swarm.set_attractor(target_x, target_y, 620.0, 140.0);
				}
			}
			MODE_GALAXY => {
				// 팔 이미터 공전. (반경이 숨 쉬듯 늘었다 줄며 나선 궤적을 만든다)
				let orbit_angle = self.elapsed_seconds * 1.35;
				let orbit_radius = 100.0 + 70.0 * (self.elapsed_seconds * 0.7).sin();
				let node_tree = self.scene_base.get_node_tree_mut();
				for (arm_index, arm_id) in self.galaxy_arm_ids.iter().enumerate() {
					let arm_angle = orbit_angle + arm_index as f64 * (std::f64::consts::PI * 2.0 / 3.0);
					let arm_x = STAGE_WIDTH * 0.5 + arm_angle.cos() * orbit_radius;
					let arm_y = STAGE_HEIGHT * 0.5 + arm_angle.sin() * orbit_radius * 0.62;
					node_tree[*arm_id].set_local_position(Vector2::create(arm_x, arm_y));
				}
			}
			_ => {}
		}
	}

	//==============================================================================
	// 복사 눌림 처리와 표시 복귀, 렌더 통계 표기.
	//==============================================================================
	fn update_overlay(&mut self, engine: &Engine, time_delta: f64) {
		let copy_label_id = self.copy_label_id.expect("copy label");
		if self.is_copy_requested.replace(false) {
			engine.write_clipboard_text(&MODE_DEFINITIONS[self.mode_index].usage.join("\n"));
			let node_tree = self.scene_base.get_node_tree_mut();
			set_label_text(node_tree, copy_label_id, "COPIED");
			set_label_color(node_tree, copy_label_id, Color::create(1.0, 1.0, 1.0, 0.95));
			self.copy_reset_seconds = 1.4;
		}
		if self.copy_reset_seconds > 0.0 {
			self.copy_reset_seconds -= time_delta;
			if self.copy_reset_seconds <= 0.0 {
				let node_tree = self.scene_base.get_node_tree_mut();
				set_label_text(node_tree, copy_label_id, "COPY");
				set_label_color(node_tree, copy_label_id, text_dim_color());
			}
		}
		// 렌더 통계. (활성 모드만, 파티클당 쿼드 6정점 2삼각형, 트레일은 세그먼트당 쿼드)
		if time_delta > 0.0 {
			self.average_delta = if self.average_delta == 0.0 { time_delta } else { self.average_delta * 0.9 + time_delta * 0.1 };
		}
		let frame_per_second = if self.average_delta > 0.0 { (1.0 / self.average_delta).round() as i64 } else { 0 };
		let total_count = self.count_particles();
		let mut vertex_count = total_count * 6;
		if self.mode_index == MODE_TRAIL {
			let node_tree = self.scene_base.get_node_tree();
			if let Some(trail_renderer) = self.trail_node_id.and_then(|trail_node_id| node_tree[trail_node_id].get_component::<TrailRenderer>()) {
				vertex_count += trail_renderer.get_point_count().saturating_sub(1) * 6;
			}
		}
		let triangle_count = vertex_count / 3;
		let node_tree = self.scene_base.get_node_tree_mut();
		set_label_text(node_tree, self.fps_label_id.expect("fps"), &format!("fps  {}", frame_per_second));
		set_label_text(node_tree, self.count_label_id.expect("count"), &format!("particles  {}", total_count));
		set_label_text(node_tree, self.vertex_label_id.expect("vertex"), &format!("vertices  {}", vertex_count));
		set_label_text(node_tree, self.triangle_label_id.expect("triangle"), &format!("triangles  {}", triangle_count));
	}
}

impl Scene for ParticleEffectsScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (제목, 목록, 무대, 열여덟 모드, 덮개)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		let _ = engine;
		self.scene_base.set_scene_background_color("rgb(20, 23, 30)");
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let title_id = create_label_node(node_tree, root_id, "Title", 24.0, 24.0, 400.0, 26.0, 18.0, text_color(), true);
		set_label_text(node_tree, title_id, "PARTICLE EFFECTS");
		self.build_effect_list(root_id);
		self.build_stage(root_id);

		let mode_builder_list: Vec<(&str, Vec<(Vector2, ParticleSystem)>)> = vec![
			("fire", ParticleEffectsScene::build_fire_mode()),
			("fountain", ParticleEffectsScene::build_fountain_mode()),
			("snow", ParticleEffectsScene::build_snow_mode()),
			("rain", ParticleEffectsScene::build_rain_mode()),
			("confetti", ParticleEffectsScene::build_confetti_mode()),
			("explosion", ParticleEffectsScene::build_explosion_mode()),
			("firework", ParticleEffectsScene::build_firework_mode()),
			("sparkle", ParticleEffectsScene::build_sparkle_mode()),
			("embers", ParticleEffectsScene::build_embers_mode()),
			("bubbles", ParticleEffectsScene::build_bubbles_mode()),
			("vortex", ParticleEffectsScene::build_vortex_mode()),
			("trail", ParticleEffectsScene::build_trail_mode()),
			("meteor", ParticleEffectsScene::build_meteor_mode()),
			("lightning", ParticleEffectsScene::build_lightning_mode()),
			("galaxy", ParticleEffectsScene::build_galaxy_mode()),
			("swarm", ParticleEffectsScene::build_swarm_mode()),
			("volcano", ParticleEffectsScene::build_volcano_mode()),
			("warp", ParticleEffectsScene::build_warp_mode()),
		];
		for (key, system_list) in mode_builder_list {
			self.add_mode(key, system_list);
		}

		// 마법 궤적의 노드에는 트레일을 함께 붙인다. 은하의 팔 노드는 공전시킨다.
		let trail_node_id = self.mode_system_ids[MODE_TRAIL][0];
		let node_tree = self.scene_base.get_node_tree_mut();
		node_tree[trail_node_id].set_pivot(Pivot::MIDDLE_CENTER);
		node_tree[trail_node_id].set_content_size(Vector2::create(8.0, 8.0));
		let core_trail = node_tree[trail_node_id].add_component(TrailRenderer::new());
		core_trail.set_time(0.65);
		core_trail.set_width(20.0, 0.0);
		core_trail.set_min_vertex_distance(2.0);
		core_trail.set_additive(true);
		core_trail.set_colors(Color::create(0.45, 0.7, 1.0, 0.85), Color::create(0.6, 0.4, 1.0, 0.0));
		self.trail_node_id = Some(trail_node_id);
		self.galaxy_arm_ids = self.mode_system_ids[MODE_GALAXY][0..3].to_vec();

		self.build_stage_overlay();
		let requested_mode = std::env::var("VANILLA_EXAMPLE_MODE").ok().and_then(|requested_key| MODE_DEFINITIONS.iter().position(|definition| definition.key == requested_key)).unwrap_or(MODE_FIRE);
		self.select_mode(requested_mode);
	}

	//==============================================================================
	// 주기적 갱신. (목록 눌림 처리, 자동 연출, 노드 갱신, 터치)
	//==============================================================================
	fn tick(&mut self, time_delta: f64) {
		let engine = Engine::get_engine().unwrap();
		if let Some(requested_index) = self.requested_mode.replace(None) {
			self.select_mode(requested_index);
		}
		self.run_automation(time_delta);
		self.scene_base.get_node_tree_mut().tick(engine, time_delta);
		self.update_overlay(engine, time_delta);
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
	engine_configuration.title = String::from("파티클 이펙트 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT);
	engine_configuration.window_size = Vector2::create(REFERENCE_WIDTH, REFERENCE_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	engine_configuration.maximum_frame_per_second = 60.0;
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(ParticleEffectsScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
