//==============================================================================
// 타일맵 쇼케이스 예제. (원본 examples/tilemapshowcase 와 같은 장면)
// - vanilla::experimental::tilemap 초안을 보인다. Tiled JSON 지도 셋(정사각, 아이소, 육각)을 읽어 그린다.
//   정사각: 층 셋(땅, 묶음 층 안의 꾸밈과 색 입힌 강조), 물결 애니메이션, 뒤집기와 돌리기 여덟 가지, 16 x 32 큰 타일, 바깥 그림 모음 타일셋,
//   자동 타일(RuleTile)로 칠한 길 층, 꾸밈 층의 충돌 모양(노란 선).
//   아이소: 마름모 지도, 블록이 위로 넘친다. 육각: 끝없는 지도의 덩이, 60 도와 120 도 돌린 화살표.
// - 아래 띠는 코드로 만든 타일맵이다. (Tilemap::rotate_tile, flip_tile_horizontally)
// - 자산은 원본 저장소 폴더에서 읽기만 한다. (timelineshowcase 예제와 같은 방식, 원본의 generate.mjs 가 만든다)
// - 환경 변수 TILEMAP_SHOWCASE_TIME=초 를 주면 애니메이션 시계를 그 시각에 멈춘다. (원본과 화면을 견줄 때, 원본은 주소의 ?time=초)
// - 실행: cargo run --example tilemapshowcase
//   화면 저장: VANILLA_CAPTURE_PATH=shots/tilemap_{frame}.png VANILLA_CAPTURE_FRAMES=30 VANILLA_CAPTURE_EXIT=1 TILEMAP_SHOWCASE_TIME=0.25
//==============================================================================

use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use vanilla::experimental::tilemap::{RuleTile, RuleTileNeighbor, RuleTileTransform, TileCollisionShape, TileCollisionShapeKind, TiledMapReader, Tilemap, TilemapLayer};
use vanilla::{Color, Component, ComponentBase, Engine, EngineConfiguration, Graphic, Image, ImageAsset, JsonAsset, NodeId, NodeTransform, NodeTree, Paint, Pivot, Promise, Scene, SceneBase, Vector2};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const EXAMPLE_ASSET_DIRECTORY: &str = "../vanilla.js/examples/tilemapshowcase/assets";
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
const MAP_FILE_LIST: [&str; 3] = ["orthogonal.tmj", "isometric.tmj", "hexagonal.tmj"];
const TILESET_FILE_LIST: [&str; 1] = ["props.tsj"];
const IMAGE_FILE_LIST: [&str; 6] = ["terrain.png", "trees.png", "chest.png", "sign.png", "isometric.png", "hexagon.png"];
// 지도마다 (패널 왼쪽, 위, 너비, 높이, 지도 노드 왼쪽, 위, 배율)
const PANEL_LAYOUT_LIST: [[f64; 7]; 3] = [
	[24.0, 40.0, 608.0, 416.0, 40.0, 56.0, 2.0],
	[664.0, 40.0, 576.0, 312.0, 680.0, 88.0, 2.0],
	[664.0, 368.0, 576.0, 336.0, 680.0, 384.0, 2.0],
];
// 자동 타일로 칠할 길 칸.
const ROAD_CELL_LIST: [(i32, i32); 11] = [(1, 6), (2, 6), (3, 6), (4, 6), (5, 6), (6, 6), (4, 7), (4, 8), (6, 7), (7, 7), (8, 7)];

//==============================================================================
// 충돌 모양을 선으로 그리는 컴포넌트. (지도 노드의 자식에 붙여 같은 좌표로 그린다)
//==============================================================================
struct CollisionOverlay {
	component_base: ComponentBase,
	shape_list: Vec<TileCollisionShape>,
}

#[vanilla::reflect]
impl CollisionOverlay {}

#[vanilla::reflect]
impl Component for CollisionOverlay {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"CollisionOverlay"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		graphic.set_stroke_color(Color::create(1.0, 0.85, 0.2, 1.0));
		for shape in self.shape_list.iter() {
			// 그리기는 엔진 Vector2 로. (충돌 모양 값과 같은 f64)
			let (shape_x, shape_y, shape_width, shape_height) = (shape.x, shape.y, shape.width, shape.height);
			let radian = shape.rotation.to_radians();
			let rotate_point = |local_x: f64, local_y: f64| Vector2::create(shape_x + local_x * radian.cos() - local_y * radian.sin(), shape_y + local_x * radian.sin() + local_y * radian.cos());
			let mut position_list: Vec<Vector2> = Vec::new();
			match shape.kind {
				TileCollisionShapeKind::Polygon | TileCollisionShapeKind::Polyline => {
					for point in shape.points.iter() {
						position_list.push(Vector2::create(shape_x + point.x, shape_y + point.y));
					}
				}
				TileCollisionShapeKind::Ellipse => {
					for step_index in 0..16 {
						let angle = step_index as f64 / 16.0 * std::f64::consts::PI * 2.0;
						position_list.push(rotate_point(shape_width * 0.5 * (1.0 + angle.cos()), shape_height * 0.5 * (1.0 + angle.sin())));
					}
				}
				TileCollisionShapeKind::Point => {
					position_list.push(Vector2::create(shape_x - 1.0, shape_y));
					position_list.push(Vector2::create(shape_x + 1.0, shape_y));
				}
				TileCollisionShapeKind::Rectangle | TileCollisionShapeKind::Capsule => {
					for (corner_x, corner_y) in [(0.0, 0.0), (shape_width, 0.0), (shape_width, shape_height), (0.0, shape_height)] {
						position_list.push(rotate_point(corner_x, corner_y));
					}
				}
			}
			if shape.kind == TileCollisionShapeKind::Polyline {
				graphic.draw_line(&position_list, 0.5);
			}
			else {
				graphic.draw_line_loop(&position_list, 0.5);
			}
		}
	}
}

//==============================================================================
// 타일맵 쇼케이스 씬.
//==============================================================================
struct TilemapShowcaseScene {
	scene_base: SceneBase,
	json_asset_list: Vec<(String, JsonAsset)>,
	image_asset_list: Vec<(String, ImageAsset)>,
	fixed_animation_time: Option<f64>,
}

impl TilemapShowcaseScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> TilemapShowcaseScene {
		let fixed_animation_time = std::env::var("TILEMAP_SHOWCASE_TIME").ok().and_then(|time_text| time_text.parse::<f64>().ok());
		TilemapShowcaseScene {
			scene_base: SceneBase::new(),
			json_asset_list: Vec::new(),
			image_asset_list: Vec::new(),
			fixed_animation_time,
		}
	}

	//==============================================================================
	// 왼쪽 위에 맞춘 노드 만들기.
	//==============================================================================
	fn create_top_left_node(node_tree: &mut NodeTree, parent_id: NodeId, name: &str, position: Vector2) -> NodeId {
		let node_id = node_tree.create_child(parent_id, name);
		let node = &mut node_tree[node_id];
		node.set_pivot(Pivot::TOP_LEFT);
		node.set_anchor(Pivot::TOP_LEFT);
		node.set_local_position(position);
		node_id
	}

	//==============================================================================
	// 정사각 지도 꾸미기. (자동 타일 길 층, 꾸밈 층의 충돌 모양)
	//==============================================================================
	fn decorate_orthogonal_map(node_tree: &mut NodeTree, map_id: NodeId, tilemap: &mut Tilemap) {
		let any = RuleTileNeighbor::Any;
		let same = RuleTileNeighbor::This;
		let other = RuleTileNeighbor::NotThis;
		let mut road_tile = RuleTile::new(17);
		road_tile.add_rule(&[any, same, any, same, same, any, same, any], 22, RuleTileTransform::Fixed);
		road_tile.add_rule(&[any, other, any, same, same, any, same, any], 21, RuleTileTransform::Rotated);
		road_tile.add_rule(&[any, other, any, same, same, any, other, any], 19, RuleTileTransform::Rotated);
		road_tile.add_rule(&[any, other, any, other, same, any, same, any], 20, RuleTileTransform::Rotated);
		road_tile.add_rule(&[any, other, any, other, same, any, other, any], 18, RuleTileTransform::Rotated);
		let mut road_layer = TilemapLayer::new("road");
		for (column, row) in ROAD_CELL_LIST.iter() {
			road_tile.paint(&mut road_layer, *column, *row);
		}
		tilemap.insert_layer(1, road_layer);

		let shape_list = match tilemap.find_layer_index("decoration") {
			Some(decoration_layer_index) => {
				tilemap.read_layer_collision_shapes(decoration_layer_index)
			}
			None => {
				Vec::new()
			}
		};
		let map_content_size = node_tree[map_id].get_content_size();
		let overlay_id = TilemapShowcaseScene::create_top_left_node(node_tree, map_id, "CollisionOverlay", Vector2::zero());
		let overlay_node = &mut node_tree[overlay_id];
		overlay_node.set_content_size(map_content_size);
		overlay_node.add_component(CollisionOverlay {
			component_base: ComponentBase::new(),
			shape_list,
		});
	}

	//==============================================================================
	// 뒤집기와 돌리기 띠. (코드로 만든 타일맵, 화살표 여덟 방향과 나무 둘)
	//==============================================================================
	fn create_flip_strip(node_tree: &mut NodeTree, root_id: NodeId, orthogonal_tilemap: &Tilemap) {
		let mut strip_map = Tilemap::new();
		strip_map.set_tile_size(16.0, 16.0);
		strip_map.set_map_size(12, 3);
		if let Some(terrain_tileset) = orthogonal_tilemap.get_tileset(0) {
			strip_map.add_tileset(terrain_tileset.clone(), 1);
		}
		if let Some(tree_tileset) = orthogonal_tilemap.get_tileset(1) {
			strip_map.add_tileset(tree_tileset.clone(), 25);
		}
		let strip_size = strip_map.read_map_pixel_size();
		let strip_layer = strip_map.create_layer("strip");
		for turn_index in 0..4 {
			let rotated_tile = Tilemap::rotate_tile(11, turn_index);
			strip_layer.set_tile(turn_index, 2, rotated_tile);
			strip_layer.set_tile(4 + turn_index, 2, Tilemap::flip_tile_horizontally(rotated_tile));
		}
		strip_layer.set_tile(9, 2, 25);
		strip_layer.set_tile(10, 2, Tilemap::flip_tile_horizontally(26));
		strip_layer.set_tile(11, 2, Tilemap::rotate_tile(25, 1));
		let strip_id = TilemapShowcaseScene::create_top_left_node(node_tree, root_id, "FlipStrip", Vector2::create(40.0, 520.0));
		let strip_node = &mut node_tree[strip_id];
		strip_node.set_local_scale(Vector2::create(3.0, 3.0));
		strip_node.set_content_size(strip_size);
		strip_node.add_component(strip_map);
	}

	//==============================================================================
	// 읽은 지도로 노드 짓기.
	//==============================================================================
	fn build_maps(&mut self, tilemap_list: Vec<Option<Tilemap>>, image_table: &HashMap<String, Rc<Image>>) {
		let read_image = |image_path: &str| -> Option<Rc<Image>> {
			image_table.get(image_path).cloned()
		};

		let fixed_animation_time = self.fixed_animation_time;
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		node_tree[root_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[root_id].set_anchor(Pivot::TOP_LEFT);
		let mut orthogonal_tilemap_copy: Option<Tilemap> = None;
		for (map_index, (file_name, tilemap)) in MAP_FILE_LIST.iter().zip(tilemap_list).enumerate() {
			let panel_layout = PANEL_LAYOUT_LIST[map_index];
			let panel_id = TilemapShowcaseScene::create_top_left_node(node_tree, root_id, "Panel", Vector2::create(panel_layout[0], panel_layout[1]));
			node_tree[panel_id].set_content_size(Vector2::create(panel_layout[2], panel_layout[3]));
			node_tree[panel_id].add_component(Paint::create_with_color(Color::create(0.11, 0.13, 0.19, 1.0)));

			let mut tilemap = match tilemap {
				Some(tilemap) => {
					tilemap
				}
				None => {
					continue;
				}
			};
			TiledMapReader::attach_images(&mut tilemap, &read_image);
			if let Some(animation_time) = fixed_animation_time {
				tilemap.set_animation_speed(0.0);
				tilemap.set_animation_time(animation_time);
			}
			let map_id = TilemapShowcaseScene::create_top_left_node(node_tree, root_id, file_name, Vector2::create(panel_layout[4], panel_layout[5]));
			node_tree[map_id].set_local_scale(Vector2::create(panel_layout[6], panel_layout[6]));
			node_tree[map_id].set_content_size(tilemap.read_map_pixel_size());
			if map_index == 0 {
				TilemapShowcaseScene::decorate_orthogonal_map(node_tree, map_id, &mut tilemap);
				orthogonal_tilemap_copy = Some(Tilemap::new());
				if let Some(strip_source) = orthogonal_tilemap_copy.as_mut() {
					for tileset_index in 0..tilemap.get_tilesets().len() {
						if let Some(tileset) = tilemap.get_tileset(tileset_index) {
							strip_source.add_tileset(tileset.clone(), tilemap.get_tileset_first_gid(tileset_index));
						}
					}
				}
			}
			node_tree[map_id].add_component(tilemap);
		}
		if let Some(orthogonal_tilemap) = orthogonal_tilemap_copy {
			TilemapShowcaseScene::create_flip_strip(node_tree, root_id, &orthogonal_tilemap);
		}
	}
}

impl Scene for TilemapShowcaseScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (지도, 바깥 타일셋, 그림을 요청해 맡긴다)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		self.scene_base.set_scene_background_color(Color::create(0.08, 0.09, 0.12, 1.0));
		for file_name in MAP_FILE_LIST.iter().chain(TILESET_FILE_LIST.iter()) {
			let mut json_asset = JsonAsset::new();
			json_asset.load(engine, file_name);
			self.scene_base.track_asset(&json_asset);
			self.json_asset_list.push((file_name.to_string(), json_asset));
		}
		for file_name in IMAGE_FILE_LIST.iter() {
			let mut image_asset = ImageAsset::new();
			image_asset.load(engine, file_name);
			self.scene_base.track_asset(&image_asset);
			self.image_asset_list.push((file_name.to_string(), image_asset));
		}
	}

	//==============================================================================
	// 초기화. (지도를 읽는 작업을 맡기고, 다 읽으면 노드를 짓는다)
	//==============================================================================
	fn initialize(&mut self, engine: &Engine) {
		let mut json_table: HashMap<String, Value> = HashMap::new();
		for (file_name, json_asset) in self.json_asset_list.iter() {
			if let Some(data) = json_asset.get_data() {
				json_table.insert(file_name.clone(), data.clone());
			}
		}
		let mut image_table: HashMap<String, Rc<Image>> = HashMap::new();
		for (file_name, image_asset) in self.image_asset_list.iter_mut() {
			if let Some(image) = image_asset.get_image() {
				image_table.insert(file_name.clone(), image);
			}
		}
		// 지도 읽기는 원본 readMap 처럼 async 라 엔진 작업으로 맡긴다. (바깥 타일셋은 가진 JSON 이라 Promise::resolved, 맡기는 자리의 첫 폴링에 끝난다)
		engine.spawn_scene_task(self, move |mut scene_context| async move {
			let read_tileset = |tileset_path: &str| -> Promise<Option<Value>> {
				Promise::resolved(json_table.get(tileset_path).cloned())
			};
			let mut tilemap_list: Vec<Option<Tilemap>> = Vec::new();
			for file_name in MAP_FILE_LIST.iter() {
				let map_data = match json_table.get(*file_name) {
					Some(map_data) => {
						map_data
					}
					None => {
						eprintln!("tilemapshowcase: {} 를 읽지 못했습니다.", file_name);
						tilemap_list.push(None);
						continue;
					}
				};
				let mut tilemap = Tilemap::new();
				match TiledMapReader::read_map(map_data, &mut tilemap, file_name, Some(&read_tileset)).await {
					Ok(_) => {
						tilemap_list.push(Some(tilemap));
					}
					Err(error) => {
						eprintln!("tilemapshowcase: {}", error);
						tilemap_list.push(None);
					}
				}
			}
			scene_context.with(move |scene, _engine| scene.build_maps(tilemap_list, &image_table));
		});
	}

	//==============================================================================
	// 이전 출력. (바탕색, 점 그림이 번지지 않게 가장 가까운 화소로)
	//==============================================================================
	fn pre_draw(&mut self, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(graphic);
		graphic.set_image_smoothing_enabled(false);
	}
}

//==============================================================================
// 진입점.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("타일맵 쇼케이스 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.window_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.asset_directory_paths = vec![PathBuf::from(EXAMPLE_ASSET_DIRECTORY)];
	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(TilemapShowcaseScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
