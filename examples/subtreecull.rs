//==============================================================================
//! 하위 트리째 그리기 건너뛰기 찍기 시험과 재기. (원본 tests/ui/subtreecullchrome.test.mjs, subtreecullparentchrome.test.mjs, scrolllistbench.mjs 의 장면)
//! - SUBTREECULL_SCENE: engine(재기 도구의 엔진 장면, 스크롤 목록 120 줄), parent(묶음 노드 바로 아래 자식 300 개 + 둥근 마스크 > 배율 > 돌림 > 스크롤 칸 144 개).
//! - SUBTREECULL_MODE: on(하위 트리 거르기 켬), off(끔), alloff(화면 밖 거르기 모두 끔).
//! - SUBTREECULL_OUTPUT: 찍은 화면과 셈을 둘 폴더. (없으면 찍지 않는다)
//! - SUBTREECULL_CHECK=1: 이 예제를 장면마다 세 길로 다시 돌려 찍은 화면 바이트, 프레임마다 그린 수가 같은지 보고 셈을 찍는다. (숨긴 창)
//! - 기즈모나 창을 보이지 않는다. 시험은 VANILLA_CAPTURE_PATH 를 걸어 창을 숨긴다.
//==============================================================================
use std::sync::atomic::{AtomicU32, Ordering};
use vanilla::core::graphic::TextAlign;
use vanilla::{Color, Component, ComponentBase, Engine, EngineConfiguration, Graphic, Mask, NodeId, NodeTransform, NodeTree, Outline, Paint, Pivot, Scene, SceneBase, UIButton, UILabel, UILineChart, UIScrollView, UISlider, UIView, Vector2, ViewScaleMode};

const VIEW_WIDTH: f64 = 640.0;
const VIEW_HEIGHT: f64 = 360.0;
const LAST_FRAME: u32 = 299;
const CAPTURE_FRAME_LIST: [u32; 6] = [30, 59, 85, 120, 250, 299];
const PUSH_COUNT_FRAME_LIST: [u32; 2] = [59, 250];
const STATIC_FIRST_FRAME: u32 = 60;
const SCROLL_FIRST_FRAME: u32 = 180;
const ROW_COUNT: usize = 120;
const ROW_HEIGHT: f64 = 34.0;
const VIEWPORT_HEIGHT: f64 = 340.0;
const PROBE_COUNT: usize = 6;

/// 이번 프레임에 변환을 건 노드 수. (PushCounter 가 센다)
static PUSH_COUNT: AtomicU32 = AtomicU32::new(0);
/// 이번 프레임에 그려진 수. (DrawProbe 가 센다)
static PROBE_DRAW_COUNT_LIST: [AtomicU32; PROBE_COUNT] = [const { AtomicU32::new(0) }; PROBE_COUNT];

//==============================================================================
// 노드에 변환을 건 수 세기. (아무것도 그리지 않는다, 원본 재기 도구의 pushState 셈 자리)
//==============================================================================
#[derive(Default)]
struct PushCounter {
	component_base: ComponentBase,
}

#[vanilla::reflect]
impl PushCounter {}

#[vanilla::reflect]
impl Component for PushCounter {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"PushCounter"
	}

	fn push_transform(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
		PUSH_COUNT.fetch_add(1, Ordering::Relaxed);
	}
}

//==============================================================================
// 그려지면 세는 것. (내용 영역 안에만 그린다고 알리고 아무것도 그리지 않는다)
//==============================================================================
struct DrawProbe {
	component_base: ComponentBase,
	probe_index: usize,
}

#[vanilla::reflect]
impl DrawProbe {}

#[vanilla::reflect]
impl Component for DrawProbe {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"DrawProbe"
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, _graphic: &mut Graphic) {
		PROBE_DRAW_COUNT_LIST[self.probe_index].fetch_add(1, Ordering::Relaxed);
	}
}

fn create_box_node(node_tree: &mut NodeTree, parent_id: NodeId, x: f64, y: f64, width: f64, height: f64) -> NodeId {
	let node_id = node_tree.create_child(parent_id, "box");
	let node = &mut node_tree[node_id];
	node.set_pivot(Pivot::TOP_LEFT);
	node.set_local_position(Vector2::create(x, y));
	node.set_content_size(Vector2::create(width, height));
	node_id
}

fn add_probe(node_tree: &mut NodeTree, node_id: NodeId, probe_index: usize) {
	node_tree.add_component(node_id, DrawProbe {
		component_base: ComponentBase::new(),
		probe_index,
	});
}

//==============================================================================
// 노드마다 PushCounter 를 단다. (자손까지)
//==============================================================================
fn add_push_counters(node_tree: &mut NodeTree, node_id: NodeId) {
	node_tree.add_component(node_id, PushCounter::default());
	let child_ids = node_tree.get_child_ids(node_id).to_vec();
	for child_id in child_ids {
		add_push_counters(node_tree, child_id);
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SceneKind {
	Engine,
	Parent,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CullMode {
	On,
	Off,
	AllOff,
}

struct SubtreeCullScene {
	scene_base: SceneBase,
	scene_kind: SceneKind,
	cull_mode: CullMode,
	output_directory: Option<String>,
	drawn_frame_count: u32,
	scroll_view_id: Option<NodeId>,
	mover_id: Option<NodeId>,
	rotated_id: Option<NodeId>,
	inner_scroll_id: Option<NodeId>,
	line_list: Vec<String>,
	probe_frame_list: Vec<Vec<u32>>,
}

impl SubtreeCullScene {
	//==============================================================================
	// 재기 도구의 엔진 장면. (스크롤 목록 120 줄, 줄마다 아이콘, 제목, 값, 단추 셋)
	//==============================================================================
	fn create_engine_scene(&mut self) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();
		let scroll_id = create_box_node(node_tree, root_id, 20.0, 10.0, 600.0, VIEWPORT_HEIGHT);
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#1d1814"));
			scroll_view.set_scroll_content_size(Vector2::create(600.0, ROW_COUNT as f64 * ROW_HEIGHT));
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		for row_index in 0..ROW_COUNT {
			let row_id = create_box_node(node_tree, content_id, 6.0, row_index as f64 * ROW_HEIGHT, 580.0, 30.0);
			let row_paint = node_tree[row_id].add_component(Paint::create_with_color(if row_index % 2 == 0 { "#4a3c30" } else { "#5a4a3c" }));
			row_paint.set_round_size(7.0);
			let icon_id = create_box_node(node_tree, row_id, 4.0, 3.0, 24.0, 24.0);
			node_tree[icon_id].add_component(Paint::create_with_color("#c89b3c")).set_round_size(5.0);
			node_tree.add_component(icon_id, Outline::new());
			let title_id = create_box_node(node_tree, row_id, 36.0, 0.0, 180.0, 30.0);
			let title_label = node_tree.add_component(title_id, UILabel::create_with_text(&format!("설정 항목 {}", row_index), 14.0)).expect("label");
			title_label.set_text_align(TextAlign::Left);
			title_label.set_text_color("#f1e9d8");
			let value_id = create_box_node(node_tree, row_id, 220.0, 0.0, 90.0, 30.0);
			let value_label = node_tree.add_component(value_id, UILabel::create_with_text(&format!("{}%", row_index * 7 % 100), 13.0)).expect("label");
			value_label.set_text_color("#d8c8a8");
			for button_index in 0..3 {
				let button_id = create_box_node(node_tree, row_id, 320.0 + button_index as f64 * 86.0, 3.0, 80.0, 24.0);
				node_tree.add_component(button_id, UIView::new()).expect("view").set_background_color(if button_index == 0 { "#2f7d4f" } else { "#7d2f3f" });
				let button_label = node_tree.add_component(button_id, UILabel::create_with_text(["켜기", "끄기", "기본"][button_index], 13.0)).expect("label");
				button_label.set_text_color("#ffffff");
				node_tree.add_component(button_id, UIButton::new());
			}
			// 들어오는 줄 셈. (12 번 줄은 처음부터 보이고, 40 번 줄은 스크롤로 들어오고, 110 번 줄은 끝까지 화면 밖)
			match row_index {
				12 => {
					add_probe(node_tree, row_id, 0);
				}
				40 => {
					add_probe(node_tree, row_id, 1);
				}
				110 => {
					add_probe(node_tree, row_id, 2);
				}
				_ => {
				}
			}
		}

		// 화면 아래 바로 밖에 있다가 85 번 프레임에 세터로 들어오는 노드.
		let mover_id = create_box_node(node_tree, root_id, 200.0, VIEW_HEIGHT + 4.0, 60.0, 20.0);
		node_tree[mover_id].add_component(Paint::create_with_color("#ff8040"));
		add_probe(node_tree, mover_id, 3);

		// 내용 영역은 화면 아래 밖이고 값 범위 밖 값의 선이 화면 안으로 올라오는 선 그래프. (내용 영역 안에서만 그린다고 알리면 화면에서 선이 사라진다)
		let chart_id = create_box_node(node_tree, root_id, 420.0, VIEW_HEIGHT + 5.0, 200.0, 50.0);
		let line_chart = node_tree.add_component(chart_id, UILineChart::new()).expect("chart");
		line_chart.set_value_range(Some(0.0), Some(10.0));
		line_chart.set_line_width(3.0);
		line_chart.set_value_list(&[0.0, 20.0, 0.0, 16.0]);

		// 내용 영역은 화면 아래 밖이고 손잡이 원만 화면 안으로 올라오는 슬라이더. (손잡이가 그리는 곳을 알리지 않으면 화면에서 사라진다)
		let slider_id = create_box_node(node_tree, root_id, 40.0, VIEW_HEIGHT + 2.0, 160.0, 10.0);
		let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
		slider.set_value(0.3);
		self.scroll_view_id = Some(scroll_id);
		self.mover_id = Some(mover_id);
		add_push_counters(node_tree, root_id);
	}

	//==============================================================================
	// 부모 좌표 먼저 거르기 장면. (묶음 노드 바로 아래 자식 300 개 스크롤, 둥근 마스크 > 배율 > 돌림 > 스크롤 칸 144 개)
	//==============================================================================
	fn create_parent_scene(&mut self) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let node_tree = self.scene_base.get_node_tree_mut();

		// 왼쪽: 묶음 노드 바로 아래 납작한 자식 300 개. (150 줄, 줄마다 이름 칸과 조절 칸)
		let scroll_id = create_box_node(node_tree, root_id, 10.0, 10.0, 280.0, 340.0);
		let content_id = {
			let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#1d1814"));
			scroll_view.set_scroll_content_size(Vector2::create(280.0, 150.0 * 28.0));
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		let group_id = create_box_node(node_tree, content_id, 0.0, 0.0, 280.0, 150.0 * 28.0);
		for row_index in 0..150 {
			let name_id = create_box_node(node_tree, group_id, 4.0, row_index as f64 * 28.0, 150.0, 24.0);
			let name_label = node_tree.add_component(name_id, UILabel::create_with_text(&format!("이름 {}", row_index), 12.0)).expect("label");
			name_label.set_text_align(TextAlign::Left);
			name_label.set_text_color("#f1e9d8");
			let control_id = create_box_node(node_tree, group_id, 160.0, row_index as f64 * 28.0 + 2.0, 110.0, 20.0);
			node_tree[control_id].add_component(Paint::create_with_color(if row_index % 2 == 0 { "#2f7d4f" } else { "#7d2f3f" })).set_round_size(6.0);
			if row_index == 30 {
				add_probe(node_tree, control_id, 0);
			}
		}

		// 오른쪽: 둥근 마스크 > 가로세로 배율 > 돌린 노드 > 스크롤 목록 칸 144 개. (자르기 겹침, 기울임)
		let mask_id = create_box_node(node_tree, root_id, 310.0, 10.0, 320.0, 340.0);
		node_tree[mask_id].add_component(Paint::create_with_color("#202830"));
		node_tree[mask_id].add_component(Mask::new()).set_round_size(40.0);
		let scaled_id = create_box_node(node_tree, mask_id, 160.0, 170.0, 0.0, 0.0);
		node_tree[scaled_id].set_local_scale(Vector2::create(1.2, 0.8));
		let rotated_id = create_box_node(node_tree, scaled_id, 0.0, 0.0, 0.0, 0.0);
		node_tree[rotated_id].set_local_rotation(35.0);
		let inner_scroll_id = create_box_node(node_tree, rotated_id, -120.0, -120.0, 240.0, 240.0);
		let inner_content_id = {
			let scroll_view = node_tree.add_component(inner_scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#30281e"));
			scroll_view.set_scroll_content_size(Vector2::create(240.0, 72.0 * 20.0));
			scroll_view.set_horizontal(false);
			scroll_view.get_content().expect("content")
		};
		for cell_index in 0..144 {
			let column_index = cell_index % 2;
			let row_index = cell_index / 2;
			let cell_id = create_box_node(node_tree, inner_content_id, 8.0 + column_index as f64 * 116.0, row_index as f64 * 20.0, 108.0, 18.0);
			node_tree[cell_id].add_component(Paint::create_with_color(if cell_index % 3 == 0 { "#c89b3c" } else { "#5a4a3c" })).set_round_size(4.0);
			if cell_index == 40 {
				add_probe(node_tree, cell_id, 1);
			}
		}

		// 화면 아래 바로 밖에 있다가 85 번 프레임에 세터로 들어오는 노드.
		let mover_id = create_box_node(node_tree, root_id, 200.0, VIEW_HEIGHT + 4.0, 60.0, 20.0);
		node_tree[mover_id].add_component(Paint::create_with_color("#ff8040"));
		add_probe(node_tree, mover_id, 3);
		self.scroll_view_id = Some(scroll_id);
		self.mover_id = Some(mover_id);
		self.rotated_id = Some(rotated_id);
		self.inner_scroll_id = Some(inner_scroll_id);
		add_push_counters(node_tree, root_id);
	}

	//==============================================================================
	// 프레임마다 바꾸는 것. (스크롤, 들어오는 노드, 돌림)
	//==============================================================================
	fn update_frame(&mut self, frame_number: u32) {
		let scroll_view_id = self.scroll_view_id.expect("scroll");
		let node_tree = self.scene_base.get_node_tree_mut();
		match self.scene_kind {
			SceneKind::Engine => {
				if frame_number >= SCROLL_FIRST_FRAME {
					let offset_y = -((frame_number - SCROLL_FIRST_FRAME) as f64) * 2.37;
					UIScrollView::set_scroll_offset_in_tree(node_tree, scroll_view_id, Vector2::create(0.0, offset_y));
				}
			}
			SceneKind::Parent => {
				if frame_number >= STATIC_FIRST_FRAME {
					let offset_y = -((frame_number - STATIC_FIRST_FRAME) as f64) * 3.1;
					UIScrollView::set_scroll_offset_in_tree(node_tree, scroll_view_id, Vector2::create(0.0, offset_y));
					let inner_scroll_id = self.inner_scroll_id.expect("inner");
					let inner_offset_y = -((frame_number - STATIC_FIRST_FRAME) as f64) * 1.7;
					UIScrollView::set_scroll_offset_in_tree(node_tree, inner_scroll_id, Vector2::create(0.0, inner_offset_y));
				}
				if frame_number == 70 {
					let rotated_id = self.rotated_id.expect("rotated");
					node_tree[rotated_id].set_local_rotation(-50.0);
				}
			}
		}
		if frame_number == 85 {
			let mover_id = self.mover_id.expect("mover");
			node_tree[mover_id].set_local_position(Vector2::create(200.0, VIEW_HEIGHT - 30.0));
		}
	}

	fn write_output(&self) {
		if let Some(output_directory) = &self.output_directory {
			let mut text = self.line_list.join("\n");
			text.push('\n');
			for (probe_index, _) in PROBE_DRAW_COUNT_LIST.iter().enumerate() {
				let probe_text: Vec<String> = self.probe_frame_list.iter().map(|count_list| count_list[probe_index].to_string()).collect();
				text.push_str(&format!("probe{}={}\n", probe_index, probe_text.join(",")));
			}
			let _ = std::fs::write(format!("{}/result.txt", output_directory), text);
		}
	}
}

impl Scene for SubtreeCullScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn load(&mut self, _engine: &Engine) {
		match self.scene_kind {
			SceneKind::Engine => {
				self.create_engine_scene();
			}
			SceneKind::Parent => {
				self.create_parent_scene();
			}
		}
	}

	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		self.drawn_frame_count += 1;
		let frame_number = self.drawn_frame_count;
		if frame_number > LAST_FRAME + 1 {
			self.write_output();
			engine.exit_application();
			return;
		}
		graphic.set_culling_enabled(self.cull_mode != CullMode::AllOff);
		graphic.set_subtree_culling_enabled(self.cull_mode == CullMode::On);
		self.update_frame(frame_number);
		if let Some(output_directory) = self.output_directory.clone() {
			if CAPTURE_FRAME_LIST.contains(&frame_number) {
				let capture_path = format!("{}/frame_{}.png", output_directory, frame_number);
				engine.request_frame_capture(engine.get_frame_number(), Box::new(move |png_bytes: &[u8]| {
					let _ = std::fs::write(&capture_path, png_bytes);
				}));
			}
		}
		PUSH_COUNT.store(0, Ordering::Relaxed);
		for probe_draw_count in PROBE_DRAW_COUNT_LIST.iter() {
			probe_draw_count.store(0, Ordering::Relaxed);
		}
		self.scene_base.draw(graphic);
		let push_count = PUSH_COUNT.load(Ordering::Relaxed);
		self.probe_frame_list.push(PROBE_DRAW_COUNT_LIST.iter().map(|probe_draw_count| probe_draw_count.load(Ordering::Relaxed)).collect());
		if PUSH_COUNT_FRAME_LIST.contains(&frame_number) {
			self.line_list.push(format!("push{}={}", frame_number, push_count));
		}
		// 건너뛴 하위 트리 수는 지난 프레임 값이다. (begin_frame_statistics 가 넘겨 둔 것)
		if PUSH_COUNT_FRAME_LIST.iter().any(|count_frame| count_frame + 1 == frame_number) {
			self.line_list.push(format!("culledSubtree{}={}", frame_number - 1, graphic.get_last_frame_culled_subtree_count()));
		}
	}
}

//==============================================================================
// 한 길 돌리기. (이 예제를 숨긴 창으로 다시 띄운다)
//==============================================================================
fn run_child(scene_name: &str, mode_name: &str, output_directory: &str) -> Result<String, String> {
	let _ = std::fs::remove_dir_all(output_directory);
	std::fs::create_dir_all(output_directory).map_err(|error| error.to_string())?;
	let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
	let status = std::process::Command::new(executable_path)
		.env("SUBTREECULL_SCENE", scene_name)
		.env("SUBTREECULL_MODE", mode_name)
		.env("SUBTREECULL_OUTPUT", output_directory)
		.env("VANILLA_CAPTURE_PATH", format!("{}/unused_{{frame}}.png", output_directory))
		.env("VANILLA_CAPTURE_FRAMES", "100000")
		.env_remove("SUBTREECULL_CHECK")
		.status()
		.map_err(|error| error.to_string())?;
	if !status.success() {
		return Err(format!("{} {} 끝 상태 {:?}", scene_name, mode_name, status.code()));
	}
	std::fs::read_to_string(format!("{}/result.txt", output_directory)).map_err(|error| error.to_string())
}

//==============================================================================
// 세 길을 견준다. (찍은 화면 바이트, 프레임마다 그린 수)
//==============================================================================
fn run_check() -> bool {
	let base_directory = std::env::var("SUBTREECULL_CHECK_DIRECTORY").unwrap_or_else(|_| String::from("target/subtreecull"));
	let mut is_passed = true;
	for scene_name in ["engine", "parent"] {
		let mut result_list: Vec<(String, String)> = Vec::new();
		for mode_name in ["on", "off", "alloff"] {
			let output_directory = format!("{}/{}_{}", base_directory, scene_name, mode_name);
			match run_child(scene_name, mode_name, &output_directory) {
				Ok(result_text) => {
					result_list.push((mode_name.to_string(), result_text));
				}
				Err(error) => {
					println!("실패: {}", error);
					return false;
				}
			}
		}
		for (mode_name, result_text) in result_list.iter() {
			let summary: Vec<&str> = result_text.lines().filter(|line| !line.starts_with("probe")).collect();
			println!("{} {}: {}", scene_name, mode_name, summary.join(" "));
		}

		// 찍은 화면은 세 길이 바이트까지 같아야 한다.
		for capture_frame in CAPTURE_FRAME_LIST {
			let read_capture = |mode_name: &str| std::fs::read(format!("{}/{}_{}/frame_{}.png", base_directory, scene_name, mode_name, capture_frame)).ok();
			let on_bytes = read_capture("on");
			let off_bytes = read_capture("off");
			let all_off_bytes = read_capture("alloff");
			if on_bytes.is_none() || on_bytes != off_bytes || on_bytes != all_off_bytes {
				println!("실패: {} {} 번 프레임 화면이 다르다 (켬 {:?} 끔 {:?} 모두 끔 {:?} 바이트)", scene_name, capture_frame, on_bytes.as_ref().map(|bytes| bytes.len()), off_bytes.as_ref().map(|bytes| bytes.len()), all_off_bytes.as_ref().map(|bytes| bytes.len()));
				is_passed = false;
			}
		}

		// 프레임마다 그린 수. (켬과 끔이 같아야 한다, 모두 끔은 화면 밖도 그린다)
		let read_probe = |result_text: &str, probe_index: usize| -> Vec<u32> {
			let prefix = format!("probe{}=", probe_index);
			result_text.lines().find(|line| line.starts_with(&prefix)).map(|line| line[prefix.len()..].split(',').filter_map(|value| value.parse::<u32>().ok()).collect()).unwrap_or_default()
		};
		for probe_index in 0..PROBE_COUNT {
			let on_list = read_probe(&result_list[0].1, probe_index);
			let off_list = read_probe(&result_list[1].1, probe_index);
			// 돌리고 기울인 칸은 끈 길(노드마다 거르기, 옮긴 경계 상자)이 실제로는 화면 밖인 칸도 그린다. 켠 길은 그보다 덜 그릴 수 있고 화면은 같다. (원본 시험도 화면만 견준다)
			if scene_name == "parent" && probe_index == 1 {
				let is_subset = on_list.len() == off_list.len() && on_list.iter().zip(off_list.iter()).all(|(on_count, off_count)| on_count <= off_count);
				let first_on = on_list.iter().position(|count| *count > 0).map(|frame_index| frame_index + 1);
				let first_off = off_list.iter().position(|count| *count > 0).map(|frame_index| frame_index + 1);
				println!("{} 돌린 칸이 처음 그려진 프레임 켬 {:?} 끔 {:?}", scene_name, first_on, first_off);
				if !is_subset {
					println!("실패: {} 돌린 칸을 켠 길이 끈 길보다 더 그렸다", scene_name);
					is_passed = false;
				}
				continue;
			}
			if on_list != off_list {
				let first_difference = on_list.iter().zip(off_list.iter()).position(|(on_count, off_count)| on_count != off_count);
				println!("실패: {} 셈 {} 의 그린 수가 켬과 끔에서 다르다 (처음 다른 프레임 {:?})", scene_name, probe_index, first_difference.map(|frame_index| frame_index + 1));
				is_passed = false;
			}
		}

		// 들어오는 노드는 85 번 프레임부터 그려지고 그 앞은 켬에서 그려지지 않는다.
		let mover_list = read_probe(&result_list[0].1, 3);
		let first_drawn_frame = mover_list.iter().position(|count| *count > 0).map(|frame_index| frame_index + 1);
		println!("{} 들어오는 노드가 처음 그려진 프레임 {:?}", scene_name, first_drawn_frame);
		if first_drawn_frame != Some(85) {
			println!("실패: {} 들어오는 노드가 85 번 프레임에 처음 그려지지 않았다", scene_name);
			is_passed = false;
		}
		let all_off_mover_list = read_probe(&result_list[2].1, 3);
		if all_off_mover_list.first().copied().unwrap_or(0) == 0 {
			println!("실패: {} 모두 끈 길이 화면 밖 노드를 그리지 않았다", scene_name);
			is_passed = false;
		}
	}
	println!("{}", if is_passed { "통과" } else { "실패" });
	is_passed
}

fn main() {
	if std::env::var("SUBTREECULL_CHECK").map(|value| value == "1").unwrap_or(false) {
		let is_passed = run_check();
		std::process::exit(if is_passed { 0 } else { 1 });
	}
	let scene_kind = match std::env::var("SUBTREECULL_SCENE").as_deref() {
		Ok("parent") => {
			SceneKind::Parent
		}
		_ => {
			SceneKind::Engine
		}
	};
	let cull_mode = match std::env::var("SUBTREECULL_MODE").as_deref() {
		Ok("off") => {
			CullMode::Off
		}
		Ok("alloff") => {
			CullMode::AllOff
		}
		_ => {
			CullMode::On
		}
	};
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("하위 트리 거르기");
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	engine.get_view_manager_mut().set_max_render_pixel_ratio(1.0);
	let subtree_cull_scene = SubtreeCullScene {
		scene_base: SceneBase::new(),
		scene_kind,
		cull_mode,
		output_directory: std::env::var("SUBTREECULL_OUTPUT").ok(),
		drawn_frame_count: 0,
		scroll_view_id: None,
		mover_id: None,
		rotated_id: None,
		inner_scroll_id: None,
		line_list: Vec::new(),
		probe_frame_list: Vec::new(),
	};
	if let Err(error) = engine.run(Box::new(subtree_cull_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
