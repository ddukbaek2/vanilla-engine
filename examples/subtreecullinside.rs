//==============================================================================
//! 보이는 사각형 안에 다 든 하위 트리 찍기 시험. (원본 tests/ui/subtreecullinsidechrome.test.mjs 의 장면과 확인)
//! - 그리기 순회가 노드의 하위 트리 경계가 보이는 사각형(화면과 걸린 자르기) 안에 다 든다고 보면 그 아래 자손은 화면 밖 견주기 없이 그리고(Graphic::is_inside_visible_bounds 가 참),
//!   그 아래에서 자르기를 새로 걸면 그 자르기 안에서 다시 견주는지 본다. (절두체 거르기의 '다 안' 표시)
//! - 왼쪽: 화면 안에 다 드는 판 아래 칸 여섯, 스크롤 목록(새 자르기, 줄 9 개, 줄마다 위와 아래 반쪽 칸), 둥근 마스크(새 자르기, 마스크 밖이지만 판 안에 놓인 칸). 프레임마다 소수 픽셀만큼 스크롤한다.
//! - 오른쪽: 30 도 돌린 판(화면 밖 칸이 하나 있어 다 안이 아니다) 아래 판(돌린 부모 좌표라 변환을 건 뒤 디바이스 좌표로 다 안을 본다)의 칸 열여섯과 돌린 스크롤 목록(줄 8 개).
//! - SUBTREECULL_MODE: on(하위 트리 거르기 켬), off(끔), alloff(화면 밖 거르기 모두 끔). SUBTREECULL_OUTPUT: 찍은 화면과 셈을 둘 폴더.
//! - SUBTREECULL_CHECK=1: 이 예제를 세 길로 다시 돌려(숨긴 창) 찍은 화면 바이트, 칸마다 그린 수, '다 안' 으로 그린 수를 견준다.
//==============================================================================
use std::sync::atomic::{AtomicU32, Ordering};
use vanilla::{Color, Component, ComponentBase, Engine, EngineConfiguration, Graphic, Mask, NodeId, NodeTransform, NodeTree, Paint, Pivot, Rect, Scene, SceneBase, UIScrollView, Vector2, ViewScaleMode};

const VIEW_WIDTH: f64 = 640.0;
const VIEW_HEIGHT: f64 = 360.0;
const CAPTURE_FRAME_LIST: [u32; 4] = [20, 41, 42, 63];
const LAST_FRAME: u32 = 66;
const COUNTER_LIMIT: usize = 64;

/// 칸마다 그린 수와 '다 안' 으로 그린 수. (DrawCounter 가 센다)
static DRAW_COUNT_LIST: [AtomicU32; COUNTER_LIMIT] = [const { AtomicU32::new(0) }; COUNTER_LIMIT];
static INSIDE_COUNT_LIST: [AtomicU32; COUNTER_LIMIT] = [const { AtomicU32::new(0) }; COUNTER_LIMIT];

//==============================================================================
// 칸마다 붙여 그린 수와 '다 안' 으로 그린 수를 세는 컴포넌트. (내용 영역 안에만 그린다)
//==============================================================================
struct DrawCounter {
	component_base: ComponentBase,
	counter_index: usize,
}

#[vanilla::reflect]
impl DrawCounter {}

#[vanilla::reflect]
impl Component for DrawCounter {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"DrawCounter"
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		DRAW_COUNT_LIST[self.counter_index].fetch_add(1, Ordering::Relaxed);
		if graphic.is_inside_visible_bounds() {
			INSIDE_COUNT_LIST[self.counter_index].fetch_add(1, Ordering::Relaxed);
		}
		graphic.set_fill_color("#ffffff");
		graphic.draw_rect(Rect::create(2.0, 2.0, 3.0, 3.0));
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CullMode {
	On,
	Off,
	AllOff,
}

struct InsideCullScene {
	scene_base: SceneBase,
	cull_mode: CullMode,
	output_directory: Option<String>,
	drawn_frame_count: u32,
	counter_name_list: Vec<String>,
	left_scroll_id: Option<NodeId>,
	rotated_scroll_id: Option<NodeId>,
	frame_line_list: Vec<String>,
}

impl InsideCullScene {
	fn create_counted_box(&mut self, parent_id: NodeId, x: f64, y: f64, width: f64, height: f64, color_text: &str, counter_name: String) -> NodeId {
		let counter_index = self.counter_name_list.len();
		self.counter_name_list.push(counter_name);
		let node_tree = self.scene_base.get_node_tree_mut();
		let node_id = create_box_node(node_tree, parent_id, x, y, width, height);
		node_tree[node_id].add_component(Paint::create_with_color(color_text));
		node_tree.add_component(node_id, DrawCounter {
			component_base: ComponentBase::new(),
			counter_index,
		});
		node_id
	}

	fn create_scene(&mut self) {
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));

		// 왼쪽 판: 화면 안에 다 든다.
		let panel_id = create_box_node(self.scene_base.get_node_tree_mut(), root_id, 20.0, 20.0, 300.0, 320.0);
		self.scene_base.get_node_tree_mut()[panel_id].add_component(Paint::create_with_color("#202838"));
		for tile_index in 0..6 {
			self.create_counted_box(panel_id, 10.0 + tile_index as f64 * 47.0, 10.0, 40.0, 40.0, "#3a6ea5", format!("판 칸 {}", tile_index));
		}
		// 스크롤 목록. (새 자르기, 줄마다 위와 아래 반쪽 칸, 가장자리에 걸친 줄은 반쪽이 자르기 밖이다)
		let left_scroll_id = create_box_node(self.scene_base.get_node_tree_mut(), panel_id, 10.0, 70.0, 280.0, 150.0);
		let left_content_id = {
			let node_tree = self.scene_base.get_node_tree_mut();
			let scroll_view = node_tree.add_component(left_scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#151a24"));
			scroll_view.set_horizontal(false);
			scroll_view.set_scroll_content_size(Vector2::create(280.0, 9.0 * 26.0));
			scroll_view.get_content().expect("content")
		};
		for row_index in 0..9 {
			let row_id = create_box_node(self.scene_base.get_node_tree_mut(), left_content_id, 4.0, row_index as f64 * 26.0, 260.0, 24.0);
			self.scene_base.get_node_tree_mut()[row_id].add_component(Paint::create_with_color(if row_index % 2 == 0 { "#4a3c30" } else { "#5a4a3c" }));
			self.create_counted_box(row_id, 8.0, 0.0, 120.0, 11.0, "#c89b3c", format!("줄 위 {}", row_index));
			self.create_counted_box(row_id, 8.0, 13.0, 120.0, 11.0, "#9bc83c", format!("줄 아래 {}", row_index));
		}
		// 둥근 마스크. (새 자르기, 오른쪽 두 칸은 마스크 밖이지만 판 안이다)
		let mask_id = create_box_node(self.scene_base.get_node_tree_mut(), panel_id, 10.0, 262.0, 200.0, 50.0);
		self.scene_base.get_node_tree_mut()[mask_id].add_component(Paint::create_with_color("#303848"));
		self.scene_base.get_node_tree_mut()[mask_id].add_component(Mask::new()).set_round_size(16.0);
		for box_index in 0..9 {
			self.create_counted_box(mask_id, -5.0 + box_index as f64 * 30.0, 5.0, 25.0, 40.0, "#a53a6e", format!("마스크 칸 {}", box_index));
		}

		// 오른쪽 돌린 판: 화면 밖 칸이 있어 다 안이 아니다. 그 아래 판은 돌린 부모 좌표라 변환을 건 뒤 디바이스 좌표로 다 안을 본다.
		let rotated_id = create_box_node(self.scene_base.get_node_tree_mut(), root_id, 480.0, 180.0, 200.0, 200.0);
		{
			let node_tree = self.scene_base.get_node_tree_mut();
			node_tree[rotated_id].set_pivot(Pivot::MIDDLE_CENTER);
			node_tree[rotated_id].set_local_rotation(30.0);
			let outside_id = create_box_node(node_tree, rotated_id, 900.0, 0.0, 20.0, 20.0);
			node_tree[outside_id].add_component(Paint::create_with_color("#ffffff"));
		}
		let inner_id = create_box_node(self.scene_base.get_node_tree_mut(), rotated_id, 0.0, 0.0, 200.0, 200.0);
		self.scene_base.get_node_tree_mut()[inner_id].add_component(Paint::create_with_color("#283820"));
		for tile_index in 0..16 {
			self.create_counted_box(inner_id, 8.0 + (tile_index % 8) as f64 * 23.0, 8.0 + (tile_index / 8) as f64 * 23.0, 20.0, 20.0, "#6ea53a", format!("돌린 판 칸 {}", tile_index));
		}
		// 돌린 자르기의 경계 상자는 넉넉하므로 좁은 칸으로 둔다. (넓으면 칸 밖 줄도 경계 상자에 걸쳐 두 길 모두 그린다)
		let rotated_scroll_id = create_box_node(self.scene_base.get_node_tree_mut(), inner_id, 10.0, 60.0, 40.0, 50.0);
		let rotated_content_id = {
			let node_tree = self.scene_base.get_node_tree_mut();
			let scroll_view = node_tree.add_component(rotated_scroll_id, UIScrollView::new()).expect("scroll view");
			scroll_view.set_background_color(Color::create_from_hex("#151a24"));
			scroll_view.set_horizontal(false);
			scroll_view.set_scroll_content_size(Vector2::create(40.0, 8.0 * 22.0));
			scroll_view.get_content().expect("content")
		};
		for row_index in 0..8 {
			self.create_counted_box(rotated_content_id, 4.0, row_index as f64 * 22.0, 30.0, 18.0, "#3ca5c8", format!("돌린 줄 {}", row_index));
		}
		self.left_scroll_id = Some(left_scroll_id);
		self.rotated_scroll_id = Some(rotated_scroll_id);
	}

	fn write_output(&self) {
		if let Some(output_directory) = &self.output_directory {
			let mut text = format!("names={}\n", self.counter_name_list.join(","));
			text.push_str(&self.frame_line_list.join("\n"));
			text.push('\n');
			let _ = std::fs::write(format!("{}/result.txt", output_directory), text);
		}
	}
}

impl Scene for InsideCullScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn load(&mut self, _engine: &Engine) {
		self.create_scene();
	}

	fn draw(&mut self, graphic: &mut Graphic) {
		let engine = Engine::get_engine().unwrap();
		self.drawn_frame_count += 1;
		let frame_number = self.drawn_frame_count;
		if frame_number > LAST_FRAME && !cfg!(target_arch = "wasm32") {
			self.write_output();
			engine.exit_application();
			return;
		}
		graphic.set_culling_enabled(self.cull_mode != CullMode::AllOff);
		graphic.set_subtree_culling_enabled(self.cull_mode == CullMode::On);

		// 21 ~ 66: 프레임마다 소수 픽셀만큼 스크롤. (줄이 자르기 가장자리에 걸쳤다 나간다)
		if frame_number > 20 {
			let node_tree = self.scene_base.get_node_tree_mut();
			UIScrollView::set_scroll_offset_in_tree(node_tree, self.left_scroll_id.expect("left"), Vector2::create(0.0, -((frame_number - 20) as f64) * 1.37));
			UIScrollView::set_scroll_offset_in_tree(node_tree, self.rotated_scroll_id.expect("rotated"), Vector2::create(0.0, -((frame_number - 20) as f64) * 1.13));
		}
		if let Some(output_directory) = self.output_directory.clone() {
			if CAPTURE_FRAME_LIST.contains(&frame_number) {
				let capture_path = format!("{}/frame_{}.png", output_directory, frame_number);
				engine.request_frame_capture(engine.get_frame_number(), Box::new(move |png_bytes: &[u8]| {
					let _ = std::fs::write(&capture_path, png_bytes);
				}));
			}
		}
		for counter_index in 0..COUNTER_LIMIT {
			DRAW_COUNT_LIST[counter_index].store(0, Ordering::Relaxed);
			INSIDE_COUNT_LIST[counter_index].store(0, Ordering::Relaxed);
		}
		self.scene_base.draw(graphic);
		let counter_count = self.counter_name_list.len();
		let draw_text: Vec<String> = (0..counter_count).map(|counter_index| DRAW_COUNT_LIST[counter_index].load(Ordering::Relaxed).to_string()).collect();
		let inside_text: Vec<String> = (0..counter_count).map(|counter_index| INSIDE_COUNT_LIST[counter_index].load(Ordering::Relaxed).to_string()).collect();
		self.frame_line_list.push(format!("frame{}={};{}", frame_number, draw_text.join(","), inside_text.join(",")));
		// 건너뛴 하위 트리 수는 지난 프레임 값이다. (begin_frame_statistics 가 넘겨 둔 것)
		if frame_number == 31 {
			self.frame_line_list.push(format!("culledSubtree30={}", graphic.get_last_frame_culled_subtree_count()));
		}
	}
}

//==============================================================================
// 한 길 돌리기. (이 예제를 숨긴 창으로 다시 띄운다)
//==============================================================================
fn run_child(mode_name: &str, output_directory: &str) -> Result<String, String> {
	let _ = std::fs::remove_dir_all(output_directory);
	std::fs::create_dir_all(output_directory).map_err(|error| error.to_string())?;
	let executable_path = std::env::current_exe().map_err(|error| error.to_string())?;
	let status = std::process::Command::new(executable_path)
		.env("SUBTREECULL_MODE", mode_name)
		.env("SUBTREECULL_OUTPUT", output_directory)
		.env("VANILLA_CAPTURE_PATH", format!("{}/unused_{{frame}}.png", output_directory))
		.env("VANILLA_CAPTURE_FRAMES", "100000")
		.env_remove("SUBTREECULL_CHECK")
		.status()
		.map_err(|error| error.to_string())?;
	if !status.success() {
		return Err(format!("{} 끝 상태 {:?}", mode_name, status.code()));
	}
	std::fs::read_to_string(format!("{}/result.txt", output_directory)).map_err(|error| error.to_string())
}

/// 한 길의 셈. (칸 이름, 프레임마다 칸별 그린 수와 '다 안' 으로 그린 수)
struct ModeResult {
	name_list: Vec<String>,
	frame_list: Vec<(Vec<u32>, Vec<u32>)>,
	culled_subtree_count: i64,
}

fn parse_result(result_text: &str) -> ModeResult {
	let mut mode_result = ModeResult {
		name_list: Vec::new(),
		frame_list: Vec::new(),
		culled_subtree_count: -1,
	};
	let parse_list = |text: &str| -> Vec<u32> {
		text.split(',').filter_map(|value| value.parse::<u32>().ok()).collect()
	};
	for line in result_text.lines() {
		if let Some(name_text) = line.strip_prefix("names=") {
			mode_result.name_list = name_text.split(',').map(String::from).collect();
		}
		else if let Some(count_text) = line.strip_prefix("culledSubtree30=") {
			mode_result.culled_subtree_count = count_text.parse::<i64>().unwrap_or(-1);
		}
		else if line.starts_with("frame") {
			if let Some((_, value_text)) = line.split_once('=') {
				if let Some((draw_text, inside_text)) = value_text.split_once(';') {
					mode_result.frame_list.push((parse_list(draw_text), parse_list(inside_text)));
				}
			}
		}
	}
	mode_result
}

//==============================================================================
// 세 길을 견준다. (원본 시험과 같은 확인)
//==============================================================================
fn run_check() -> bool {
	let base_directory = std::env::var("SUBTREECULL_CHECK_DIRECTORY").unwrap_or_else(|_| String::from("target/subtreecullinside"));
	let mut result_list: Vec<ModeResult> = Vec::new();
	for mode_name in ["on", "off", "alloff"] {
		match run_child(mode_name, &format!("{}/{}", base_directory, mode_name)) {
			Ok(result_text) => {
				result_list.push(parse_result(&result_text));
			}
			Err(error) => {
				println!("실패: {}", error);
				return false;
			}
		}
	}
	let mut is_passed = true;
	let mut check = |is_true: bool, text: String| {
		println!("{} {}", if is_true { "통과" } else { "실패" }, text);
		if !is_true {
			is_passed = false;
		}
	};

	// 찍은 화면은 세 길이 바이트까지 같아야 한다.
	for capture_frame in CAPTURE_FRAME_LIST {
		let read_capture = |mode_name: &str| std::fs::read(format!("{}/{}/frame_{}.png", base_directory, mode_name, capture_frame)).ok();
		let on_bytes = read_capture("on");
		check(on_bytes.is_some() && on_bytes == read_capture("off") && on_bytes == read_capture("alloff"), format!("{} 번 프레임 켬, 끔, 모두 끔 화면 바이트 같음", capture_frame));
	}
	let on_result = &result_list[0];
	let off_result = &result_list[1];
	let name_list = &on_result.name_list;
	check(name_list.len() == 6 + 18 + 9 + 16 + 8, format!("칸 수 {}", name_list.len()));
	check(on_result.frame_list.len() == LAST_FRAME as usize && off_result.frame_list.len() == LAST_FRAME as usize, format!("프레임 수 켬 {} 끔 {}", on_result.frame_list.len(), off_result.frame_list.len()));
	let find_index_list = |prefix: &str| -> Vec<usize> {
		name_list.iter().enumerate().filter(|(_, name)| name.starts_with(prefix)).map(|(counter_index, _)| counter_index).collect()
	};

	// 켬과 끔이 프레임마다 같은 칸을 그린다. (돌린 스크롤 목록은 켠 길이 그린 줄을 끈 길도 그리면 된다)
	let mut mismatch_text = String::new();
	let mut rotated_mismatch_text = String::new();
	for (frame_index, ((on_draw_list, _), (off_draw_list, _))) in on_result.frame_list.iter().zip(off_result.frame_list.iter()).enumerate() {
		for counter_index in 0..name_list.len() {
			let count_text = format!("{} 프레임 {}: 켬 {}, 끔 {}", frame_index + 1, name_list[counter_index], on_draw_list[counter_index], off_draw_list[counter_index]);
			if name_list[counter_index].starts_with("돌린 줄 ") {
				if rotated_mismatch_text.is_empty() && on_draw_list[counter_index] > off_draw_list[counter_index] {
					rotated_mismatch_text = count_text;
				}
			}
			else if mismatch_text.is_empty() && on_draw_list[counter_index] != off_draw_list[counter_index] {
				mismatch_text = count_text;
			}
		}
	}
	check(mismatch_text.is_empty(), format!("켬과 끔이 프레임마다 같은 칸을 그림 {}", mismatch_text));
	check(rotated_mismatch_text.is_empty(), format!("돌린 스크롤 목록에서 켠 길이 그린 줄은 끈 길도 그림 {}", rotated_mismatch_text));

	// 마지막 프레임에 그린 칸. (새 자르기 밖 칸은 그리지 않는다)
	let last_draw_list = on_result.frame_list.last().map(|(draw_list, _)| draw_list.clone()).unwrap_or_default();
	let row_index_list = find_index_list("줄 ");
	let drawn_row_count = row_index_list.iter().filter(|counter_index| last_draw_list[**counter_index] > 0).count();
	let mask_index_list = find_index_list("마스크 칸 ");
	let rotated_row_index_list = find_index_list("돌린 줄 ");
	let drawn_rotated_row_count = rotated_row_index_list.iter().filter(|counter_index| last_draw_list[**counter_index] > 0).count();
	check(drawn_row_count > 6 && drawn_row_count < 18, format!("스크롤 목록 칸은 보이는 것만 그림 ({} / 18)", drawn_row_count));
	check(last_draw_list[mask_index_list[7]] == 0 && last_draw_list[mask_index_list[8]] == 0, String::from("마스크 밖 오른쪽 두 칸은 그리지 않음"));
	check(last_draw_list[mask_index_list[0]] > 0 && last_draw_list[mask_index_list[4]] > 0, String::from("마스크에 걸친 칸과 안 칸은 그림"));
	check(drawn_rotated_row_count >= 2 && drawn_rotated_row_count < 8, format!("돌린 스크롤 목록 칸은 보이는 것만 그림 ({} / 8)", drawn_rotated_row_count));
	check(on_result.culled_subtree_count > 0, format!("켠 길은 '다 안' 아래 새 자르기 밖 하위 트리를 건너뜀 ({})", on_result.culled_subtree_count));

	// '다 안' 으로 그린 칸. (판 아래 칸은 프레임마다, 가장자리에 걸친 줄의 칸은 아님)
	let is_all_inside = |prefix: &str| -> bool {
		on_result.frame_list.iter().all(|(draw_list, inside_list)| find_index_list(prefix).iter().all(|counter_index| draw_list[*counter_index] == 1 && inside_list[*counter_index] == 1))
	};
	check(is_all_inside("판 칸 "), String::from("판 아래 칸은 프레임마다 '다 안' 으로 그림"));
	check(is_all_inside("돌린 판 칸 "), String::from("돌린 판 아래 칸도 프레임마다 '다 안' 으로 그림 (변환 뒤 디바이스 좌표로 본다)"));
	// 원본처럼 처음부터 마지막 프레임까지 모은 수로 센다.
	let total_draw_list: Vec<u32> = (0..name_list.len()).map(|counter_index| on_result.frame_list.iter().map(|(draw_list, _)| draw_list[counter_index]).sum()).collect();
	let total_inside_list: Vec<u32> = (0..name_list.len()).map(|counter_index| on_result.frame_list.iter().map(|(_, inside_list)| inside_list[counter_index]).sum()).collect();
	let partial_row_count = row_index_list.iter().filter(|counter_index| total_draw_list[**counter_index] > total_inside_list[**counter_index]).count();
	let inside_row_count = row_index_list.iter().filter(|counter_index| total_inside_list[**counter_index] > 0).count();
	println!("건너뛴 하위 트리(30) {}, 그린 줄 칸 {}, '다 안' 줄 칸 {}, 걸친 줄 칸 {}, 돌린 줄 칸 {}", on_result.culled_subtree_count, drawn_row_count, inside_row_count, partial_row_count, drawn_rotated_row_count);
	check(partial_row_count > 0, format!("가장자리에 걸친 줄의 칸은 '다 안' 이 아닌 프레임이 있음 ({})", partial_row_count));
	check(inside_row_count > 0, format!("새 자르기 안에 다 든 줄의 칸은 다시 '다 안' 으로 그림 ({})", inside_row_count));
	let off_inside_count: u32 = off_result.frame_list.iter().map(|(_, inside_list)| inside_list.iter().sum::<u32>()).sum();
	check(off_inside_count == 0, format!("끈 길은 '다 안' 이 없음 ({})", off_inside_count));
	println!("{}", if is_passed { "통과" } else { "실패" });
	is_passed
}

fn main() {
	if std::env::var("SUBTREECULL_CHECK").map(|value| value == "1").unwrap_or(false) {
		let is_passed = run_check();
		std::process::exit(if is_passed { 0 } else { 1 });
	}
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
	engine_configuration.title = String::from("다 안 거르기");
	// 원본 시험처럼 디바이스 픽셀이 기준 단위의 1/3 이 되게 1920 x 1080 으로 그린다. ('다 안' 은 한 디바이스 픽셀 좁혀 본다)
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH * 3.0, VIEW_HEIGHT * 3.0);
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	let engine = Engine::new(engine_configuration);
	engine.get_view_manager_mut().set_max_render_pixel_ratio(1.0);
	let inside_cull_scene = InsideCullScene {
		scene_base: SceneBase::new(),
		cull_mode,
		output_directory: std::env::var("SUBTREECULL_OUTPUT").ok(),
		drawn_frame_count: 0,
		counter_name_list: Vec::new(),
		left_scroll_id: None,
		rotated_scroll_id: None,
		frame_line_list: Vec::new(),
	};
	if let Err(error) = engine.run(Box::new(inside_cull_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
