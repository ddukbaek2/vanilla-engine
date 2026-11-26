//==============================================================================
//! Tiled JSON 지도 읽기. (원본 src/experimental/tilemap/tiledmapreader.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - Tiled 의 JSON 지도(.tmj)와 타일셋(.tsj)을 읽어 Tilemap 을 채운다. (원문: <https://doc.mapeditor.org/en/stable/reference/json-map-format/>)
//! - 읽는 것: 배치 넷(orthogonal, isometric, staggered, hexagonal), 지도와 층과 타일셋과 타일의 속성, 타일 층(배열, base64, base64 + zlib, gzip),
//!   끝없는 지도의 덩이(chunks), 묶음 층(group, 오프셋과 불투명도와 색과 보이기를 아래 층에 곱한다), 물체 층(TiledObjectLayer 로 돌려준다),
//!   타일셋의 격자와 그림 모음, 타일 오프셋, 그리는 크기, 채우기, 타일 애니메이션, 타일 충돌 모양(타일의 objectgroup), 층 섞기(mode).
//! - 범위 밖: zstd 압축(Err), oblique 배치(Err), 그림 층, 물체 템플릿, 왕 셋(wangsets), 시차(parallax), 투명색(transparentcolor), 타일셋 격자의 아이소 방향.
//! - 그림은 읽지 않는다. 타일셋에 그림 경로만 적고, collect_image_paths 로 경로를 모아 읽은 뒤 attach_images 로 건다.
//! - 원본과 다른 점: 원본 readMap 은 zlib, gzip 을 브라우저 DecompressionStream 으로 풀어서 비동기다. 러스트 판은 flate2 로 그 자리에서 끝난다.
//!   던지는 대신 Err(글) 을 돌려준다. (글은 원본 Error 글과 같다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::BlendMode;
use crate::core::image::Image;
use crate::experimental::tilemap::tilemap::{Tilemap, TilemapOrientation, TilemapRenderOrder, TilemapStaggerAxis, TilemapStaggerIndex};
use crate::experimental::tilemap::tilemaplayer::TilemapLayer;
use crate::experimental::tilemap::tileset::{TileAnimationFrame, TileCollisionShape, TileCollisionShapeKind, TileFillMode, TileRenderSize, Tileset};
use serde_json::{Map, Value};
use std::io::Read;
use std::rc::Rc;

//==============================================================================
/// 읽은 물체 하나. (Tiled 물체)
///
/// # 예제
/// 물체 자료 만들기
/// ```rust
/// use vanilla::experimental::tilemap::{TileCollisionShape, TileCollisionShapeKind, TiledObject};
///
/// let mut tiled_object = TiledObject::new();
/// tiled_object.name = String::from("chest");
/// tiled_object.shape = TileCollisionShape::create_rectangle(32.0, 48.0, 16.0, 16.0);
/// assert_eq!((tiled_object.shape.kind, tiled_object.tile), (TileCollisionShapeKind::Rectangle, 0));
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TiledObject {
	/// 지도 안의 물체 번호.
	pub id: u32,
	/// 이름.
	pub name: String,
	/// 종류. (Tiled 의 class)
	pub object_type: String,
	/// 타일 물체면 타일 값. (뒤집기 비트를 포함한 gid, 아니면 0, Tiled 처럼 x, y 는 그림의 왼쪽 아래)
	pub tile: u32,
	/// 모양과 자리. (지도 픽셀 좌표, 글 물체는 사각형)
	pub shape: TileCollisionShape,
	/// 보이는지.
	pub is_visible: bool,
	/// 속성.
	pub properties: Map<String, Value>,
}

impl TiledObject {
	//==============================================================================
	/// 생성. (빈 사각형 물체)
	///
	/// # 돌려줌
	/// 새 물체.
	//==============================================================================
	pub fn new() -> TiledObject {
		TiledObject {
			id: 0,
			name: String::new(),
			object_type: String::new(),
			tile: 0,
			shape: TileCollisionShape::new(TileCollisionShapeKind::Rectangle),
			is_visible: true,
			properties: Map::new(),
		}
	}
}

impl Default for TiledObject {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TiledObject {
		TiledObject::new()
	}
}

//==============================================================================
/// 읽은 물체 층 하나. (Tiled objectgroup)
///
/// # 예제
/// 물체 층에서 이름으로 찾기
/// ```rust
/// use vanilla::experimental::tilemap::{TiledObject, TiledObjectLayer};
///
/// let mut object_layer = TiledObjectLayer::new();
/// let mut door = TiledObject::new();
/// door.name = String::from("door");
/// object_layer.objects.push(door);
/// assert!(object_layer.objects.iter().any(|tiled_object| tiled_object.name == "door"));
/// assert_eq!(object_layer.opacity, 1.0);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TiledObjectLayer {
	/// 이름.
	pub name: String,
	/// 보이는지. (묶음 층의 보이기를 곱한 것)
	pub is_visible: bool,
	/// 불투명도. (묶음 층의 불투명도를 곱한 것)
	pub opacity: f32,
	/// 자리 오프셋. (픽셀, 묶음 층의 오프셋을 더한 것)
	pub offset: Vector2,
	/// 속성.
	pub properties: Map<String, Value>,
	/// 물체 목록. (파일 차례)
	pub objects: Vec<TiledObject>,
}

impl TiledObjectLayer {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 물체 층.
	//==============================================================================
	pub fn new() -> TiledObjectLayer {
		TiledObjectLayer {
			name: String::new(),
			is_visible: true,
			opacity: 1.0,
			offset: Vector2::zero(),
			properties: Map::new(),
			objects: Vec::new(),
		}
	}
}

impl Default for TiledObjectLayer {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TiledObjectLayer {
		TiledObjectLayer::new()
	}
}

//==============================================================================
/// 읽은 지도에서 Tilemap 에 담지 않는 것. (지도 속성, 배경색, 물체 층)
///
/// # 예제
/// 빈 결과
/// ```rust
/// use vanilla::experimental::tilemap::TiledMap;
///
/// let tiled_map = TiledMap::new();
/// assert!(tiled_map.properties.is_empty() && tiled_map.background_color.is_none() && tiled_map.object_layers.is_empty());
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TiledMap {
	/// 지도 속성.
	pub properties: Map<String, Value>,
	/// 배경색. (없으면 None)
	pub background_color: Option<Color>,
	/// 물체 층. (파일 차례)
	pub object_layers: Vec<TiledObjectLayer>,
}

impl TiledMap {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 결과.
	//==============================================================================
	pub fn new() -> TiledMap {
		TiledMap {
			properties: Map::new(),
			background_color: None,
			object_layers: Vec::new(),
		}
	}
}

impl Default for TiledMap {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TiledMap {
		TiledMap::new()
	}
}

//==============================================================================
/// 묶음 층이 아래로 내리는 값. (오프셋은 더하고 불투명도, 보이기, 색은 곱한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct GroupState {
	offset_x: f32,
	offset_y: f32,
	opacity: f32,
	is_visible: bool,
	color: Color,
}

//==============================================================================
/// Tiled JSON 지도 읽기. (함수 묶음)
///
/// # 예제
/// 지도 JSON 을 읽어 타일맵 채우기
/// ```rust
/// use vanilla::experimental::tilemap::{TiledMapReader, Tilemap};
///
/// let map_data = serde_json::json!({
/// 	"type": "map", "orientation": "orthogonal", "renderorder": "right-down", "infinite": false,
/// 	"width": 3, "height": 2, "tilewidth": 16, "tileheight": 16,
/// 	"properties": [{ "name": "music", "type": "string", "value": "field" }],
/// 	"tilesets": [{ "firstgid": 1, "name": "terrain", "image": "terrain.png", "imagewidth": 64, "imageheight": 32, "tilewidth": 16, "tileheight": 16, "columns": 4, "tilecount": 8, "margin": 0, "spacing": 0 }],
/// 	"layers": [
/// 		{ "type": "tilelayer", "name": "ground", "width": 3, "height": 2, "data": [1, 2, 3, 4, 2147483653u32, 6], "opacity": 1, "visible": true, "x": 0, "y": 0 },
/// 		{ "type": "objectgroup", "name": "spawn", "objects": [{ "id": 1, "name": "player", "type": "start", "x": 8, "y": 8, "width": 0, "height": 0, "point": true, "rotation": 0, "visible": true }] }
/// 	]
/// });
/// let mut tilemap = Tilemap::new();
/// let tiled_map = TiledMapReader::read_map(&map_data, &mut tilemap, "maps/field.tmj", None).expect("map");
/// assert_eq!((tilemap.get_layer_count(), tilemap.get_layer(0).expect("layer").get_tile(1, 1)), (1, 2147483653));
/// assert_eq!(tiled_map.properties["music"], "field");
/// assert_eq!(tiled_map.object_layers[0].objects[0].name, "player");
/// assert_eq!(TiledMapReader::collect_image_paths(&tilemap), vec![String::from("maps/terrain.png")]);
/// ```
//==============================================================================
pub struct TiledMapReader;

impl TiledMapReader {
	//==============================================================================
	/// 지도 읽기.
	/// - tilemap 의 배치, 칸 크기, 지도 크기를 정하고 타일셋과 타일 층을 더한다. (이미 있던 층과 타일셋 뒤에 더한다)
	/// - 바깥 타일셋(source)은 read_tileset_handler(지도 경로에서 푼 경로) 가 돌려준 JSON 으로 읽는다.
	///
	/// # 인자
	/// - `map_data`: 지도 JSON 을 푼 값.
	/// - `tilemap`: 채울 타일맵.
	/// - `map_path`: 지도 파일 경로. (타일셋과 그림의 상대 경로를 푸는 기준)
	/// - `read_tileset_handler`: 바깥 타일셋 경로를 받아 그 JSON 을 돌려주는 함수. (없으면 None)
	///
	/// # 돌려줌
	/// 지도 속성, 배경색, 물체 층. 지원하지 않는 배치나 압축, 바깥 타일셋을 못 읽으면 Err.
	//==============================================================================
	pub fn read_map(map_data: &Value, tilemap: &mut Tilemap, map_path: &str, read_tileset_handler: Option<&dyn Fn(&str) -> Option<Value>>) -> Result<TiledMap, String> {
		let mut tiled_map = TiledMap::new();
		let orientation_name = TiledMapReader::read_string(map_data, "orientation", "orthogonal");
		let orientation = match orientation_name {
			"orthogonal" => {
				TilemapOrientation::Orthogonal
			}
			"isometric" => {
				TilemapOrientation::Isometric
			}
			"staggered" => {
				TilemapOrientation::Staggered
			}
			"hexagonal" => {
				TilemapOrientation::Hexagonal
			}
			_ => {
				return Err(format!("TiledMapReader: unsupported orientation {}", orientation_name));
			}
		};
		tilemap.set_orientation(orientation);
		tilemap.set_tile_size(TiledMapReader::read_number(map_data, "tilewidth", 32.0) as u32, TiledMapReader::read_number(map_data, "tileheight", 32.0) as u32);
		tilemap.set_map_size(TiledMapReader::read_number(map_data, "width", 0.0) as u32, TiledMapReader::read_number(map_data, "height", 0.0) as u32);
		tilemap.set_hex_side_length(TiledMapReader::read_number(map_data, "hexsidelength", 0.0) as u32);
		tilemap.set_stagger_axis(if TiledMapReader::read_string(map_data, "staggeraxis", "y") == "x" {
			TilemapStaggerAxis::X
		}
		else {
			TilemapStaggerAxis::Y
		});
		tilemap.set_stagger_index(if TiledMapReader::read_string(map_data, "staggerindex", "odd") == "even" {
			TilemapStaggerIndex::Even
		}
		else {
			TilemapStaggerIndex::Odd
		});
		tilemap.set_render_order(match TiledMapReader::read_string(map_data, "renderorder", "right-down") {
			"right-up" => {
				TilemapRenderOrder::RightUp
			}
			"left-down" => {
				TilemapRenderOrder::LeftDown
			}
			"left-up" => {
				TilemapRenderOrder::LeftUp
			}
			_ => {
				TilemapRenderOrder::RightDown
			}
		});
		tiled_map.background_color = TiledMapReader::parse_color(TiledMapReader::read_string(map_data, "backgroundcolor", ""));
		tiled_map.properties = TiledMapReader::read_properties(map_data);

		// 타일셋.
		if let Some(tileset_entry_list) = map_data.get("tilesets").and_then(Value::as_array) {
			for tileset_entry in tileset_entry_list.iter() {
				let first_gid = TiledMapReader::read_number(tileset_entry, "firstgid", 0.0) as u32;
				let source_path = TiledMapReader::read_string(tileset_entry, "source", "");
				let tileset = if !source_path.is_empty() {
					let tileset_path = TiledMapReader::resolve_path(map_path, source_path);
					let tileset_data = read_tileset_handler.and_then(|read_tileset| read_tileset(&tileset_path));
					match tileset_data {
						Some(tileset_data) => {
							TiledMapReader::read_tileset(&tileset_data, &tileset_path)
						}
						None => {
							return Err(format!("TiledMapReader: cannot read tileset {}", tileset_path));
						}
					}
				}
				else {
					TiledMapReader::read_tileset(tileset_entry, map_path)
				};
				tilemap.add_tileset(tileset, first_gid);
			}
		}

		// 층. (묶음 층은 펼친다)
		let group_state = GroupState {
			offset_x: 0.0,
			offset_y: 0.0,
			opacity: 1.0,
			is_visible: true,
			color: Color::white(),
		};
		if let Some(layer_entry_list) = map_data.get("layers").and_then(Value::as_array) {
			TiledMapReader::read_layer_list(layer_entry_list, tilemap, &mut tiled_map, group_state)?;
		}
		Ok(tiled_map)
	}

	//==============================================================================
	/// 층 목록 읽기. (묶음 층은 그 값을 곱하고 더해 아래로 내린다)
	//==============================================================================
	fn read_layer_list(layer_entry_list: &[Value], tilemap: &mut Tilemap, tiled_map: &mut TiledMap, group_state: GroupState) -> Result<(), String> {
		for layer_entry in layer_entry_list.iter() {
			let layer_type = TiledMapReader::read_string(layer_entry, "type", "");
			let offset_x = group_state.offset_x + TiledMapReader::read_number(layer_entry, "offsetx", 0.0) as f32;
			let offset_y = group_state.offset_y + TiledMapReader::read_number(layer_entry, "offsety", 0.0) as f32;
			let opacity = group_state.opacity * TiledMapReader::read_number(layer_entry, "opacity", 1.0) as f32;
			let is_visible = group_state.is_visible && layer_entry.get("visible").and_then(Value::as_bool) != Some(false);
			let mut color = group_state.color;
			if let Some(tint_color) = TiledMapReader::parse_color(TiledMapReader::read_string(layer_entry, "tintcolor", "")) {
				color = Color::create(color.red * tint_color.red, color.green * tint_color.green, color.blue * tint_color.blue, color.alpha * tint_color.alpha);
			}
			let name = TiledMapReader::read_string(layer_entry, "name", "");
			let properties = TiledMapReader::read_properties(layer_entry);
			match layer_type {
				"tilelayer" => {
					let mut layer = TilemapLayer::new(name);
					layer.set_visible(is_visible);
					layer.set_opacity(opacity);
					layer.set_color(color);
					layer.set_offset(Vector2::create(offset_x, offset_y));
					let mode_name = TiledMapReader::read_string(layer_entry, "mode", "normal");
					let blend_mode_name = match mode_name {
						"normal" => {
							"source-over"
						}
						"add" => {
							"lighter"
						}
						_ => {
							mode_name
						}
					};
					layer.set_blend_mode(BlendMode::from_name(blend_mode_name).unwrap_or(BlendMode::Normal));
					for (property_name, property_value) in properties.iter() {
						layer.set_property(property_name, property_value.clone());
					}
					let encoding = TiledMapReader::read_string(layer_entry, "encoding", "csv");
					let compression = TiledMapReader::read_string(layer_entry, "compression", "");
					if let Some(chunk_entry_list) = layer_entry.get("chunks").and_then(Value::as_array) {
						for chunk_entry in chunk_entry_list.iter() {
							let chunk_width = TiledMapReader::read_number(chunk_entry, "width", 0.0) as i32;
							let chunk_height = TiledMapReader::read_number(chunk_entry, "height", 0.0) as i32;
							let chunk_x = TiledMapReader::read_number(chunk_entry, "x", 0.0) as i32;
							let chunk_y = TiledMapReader::read_number(chunk_entry, "y", 0.0) as i32;
							let chunk_data = chunk_entry.get("data").unwrap_or(&Value::Null);
							let chunk_tile_list = TiledMapReader::read_layer_data(chunk_data, encoding, compression, (chunk_width * chunk_height).max(0) as usize)?;
							layer.set_tiles(chunk_x, chunk_y, chunk_width, chunk_height, &chunk_tile_list);
						}
					}
					else {
						let layer_width = TiledMapReader::read_number(layer_entry, "width", 0.0) as i32;
						let layer_height = TiledMapReader::read_number(layer_entry, "height", 0.0) as i32;
						let layer_data = layer_entry.get("data").unwrap_or(&Value::Null);
						let tile_list = TiledMapReader::read_layer_data(layer_data, encoding, compression, (layer_width * layer_height).max(0) as usize)?;
						layer.set_tiles(0, 0, layer_width, layer_height, &tile_list);
					}
					tilemap.add_layer(layer);
				}
				"objectgroup" => {
					let mut object_layer = TiledObjectLayer::new();
					object_layer.name = String::from(name);
					object_layer.is_visible = is_visible;
					object_layer.opacity = opacity;
					object_layer.offset = Vector2::create(offset_x, offset_y);
					object_layer.properties = properties;
					if let Some(object_entry_list) = layer_entry.get("objects").and_then(Value::as_array) {
						for object_entry in object_entry_list.iter() {
							object_layer.objects.push(TiledMapReader::read_object(object_entry));
						}
					}
					tiled_map.object_layers.push(object_layer);
				}
				"group" => {
					let child_state = GroupState {
						offset_x,
						offset_y,
						opacity,
						is_visible,
						color,
					};
					if let Some(child_entry_list) = layer_entry.get("layers").and_then(Value::as_array) {
						TiledMapReader::read_layer_list(child_entry_list, tilemap, tiled_map, child_state)?;
					}
				}
				_ => {
				}
			}
		}
		Ok(())
	}

	//==============================================================================
	/// 타일셋 읽기. (.tsj 나 지도 안의 타일셋)
	///
	/// # 인자
	/// - `tileset_data`: 타일셋 JSON 을 푼 값.
	/// - `tileset_path`: 타일셋 파일 경로. (그림의 상대 경로를 푸는 기준, 지도 안이면 지도 경로)
	///
	/// # 돌려줌
	/// 새 타일셋. (그림은 비어 있고 경로만 적혀 있다)
	//==============================================================================
	pub fn read_tileset(tileset_data: &Value, tileset_path: &str) -> Tileset {
		let mut tileset = Tileset::new();
		tileset.set_name(TiledMapReader::read_string(tileset_data, "name", ""));
		let tile_width = TiledMapReader::read_number(tileset_data, "tilewidth", 0.0);
		let tile_height = TiledMapReader::read_number(tileset_data, "tileheight", 0.0);
		tileset.set_tile_size(tile_width as u32, tile_height as u32);
		tileset.set_margin(TiledMapReader::read_number(tileset_data, "margin", 0.0) as u32);
		tileset.set_spacing(TiledMapReader::read_number(tileset_data, "spacing", 0.0) as u32);
		tileset.set_columns(TiledMapReader::read_number(tileset_data, "columns", 0.0) as u32);
		tileset.set_tile_count(TiledMapReader::read_number(tileset_data, "tilecount", 0.0) as u32);
		let image_name = TiledMapReader::read_string(tileset_data, "image", "");
		if !image_name.is_empty() {
			tileset.set_image_path(&TiledMapReader::resolve_path(tileset_path, image_name));
		}
		if let Some(tile_offset) = tileset_data.get("tileoffset").filter(|value| value.is_object()) {
			tileset.set_tile_offset(Vector2::create(TiledMapReader::read_number(tile_offset, "x", 0.0) as f32, TiledMapReader::read_number(tile_offset, "y", 0.0) as f32));
		}
		tileset.set_tile_render_size(if TiledMapReader::read_string(tileset_data, "tilerendersize", "tile") == "grid" {
			TileRenderSize::Grid
		}
		else {
			TileRenderSize::Tile
		});
		tileset.set_fill_mode(if TiledMapReader::read_string(tileset_data, "fillmode", "stretch") == "preserve-aspect-fit" {
			TileFillMode::PreserveAspectFit
		}
		else {
			TileFillMode::Stretch
		});
		for (property_name, property_value) in TiledMapReader::read_properties(tileset_data).iter() {
			tileset.set_property(property_name, property_value.clone());
		}

		// 타일마다.
		if let Some(tile_entry_list) = tileset_data.get("tiles").and_then(Value::as_array) {
			for tile_entry in tile_entry_list.iter() {
				let tile_id = TiledMapReader::read_number(tile_entry, "id", 0.0) as u32;
				let tile_image_name = TiledMapReader::read_string(tile_entry, "image", "");
				if !tile_image_name.is_empty() {
					tileset.set_tile_image_path(tile_id, &TiledMapReader::resolve_path(tileset_path, tile_image_name));
				}
				let has_sub_rectangle = tile_entry.get("x").is_some() || tile_entry.get("y").is_some() || tile_entry.get("width").is_some() || tile_entry.get("height").is_some();
				if !tile_image_name.is_empty() || has_sub_rectangle {
					let image_width = TiledMapReader::read_number(tile_entry, "imagewidth", tile_width);
					let image_height = TiledMapReader::read_number(tile_entry, "imageheight", tile_height);
					let rect_x = TiledMapReader::read_number(tile_entry, "x", 0.0);
					let rect_y = TiledMapReader::read_number(tile_entry, "y", 0.0);
					let rect_width = TiledMapReader::read_number(tile_entry, "width", image_width);
					let rect_height = TiledMapReader::read_number(tile_entry, "height", image_height);
					tileset.set_tile_image_rect(tile_id, Some(Rect::create(rect_x as f32, rect_y as f32, rect_width as f32, rect_height as f32)));
				}
				let mut tile_type = TiledMapReader::read_string(tile_entry, "type", "");
				if tile_type.is_empty() {
					tile_type = TiledMapReader::read_string(tile_entry, "class", "");
				}
				if !tile_type.is_empty() {
					tileset.set_tile_type(tile_id, tile_type);
				}
				for (property_name, property_value) in TiledMapReader::read_properties(tile_entry).iter() {
					tileset.set_tile_property(tile_id, property_name, property_value.clone());
				}
				if let Some(frame_entry_list) = tile_entry.get("animation").and_then(Value::as_array) {
					if !frame_entry_list.is_empty() {
						let frame_list: Vec<TileAnimationFrame> = frame_entry_list.iter().map(|frame_entry| TileAnimationFrame::new(TiledMapReader::read_number(frame_entry, "tileid", 0.0) as u32, TiledMapReader::read_number(frame_entry, "duration", 0.0) as u32)).collect();
						tileset.set_tile_animation(tile_id, &frame_list);
					}
				}
				if let Some(object_entry_list) = tile_entry.get("objectgroup").and_then(|object_group| object_group.get("objects")).and_then(Value::as_array) {
					let shape_list: Vec<TileCollisionShape> = object_entry_list.iter().map(|object_entry| TiledMapReader::read_object(object_entry).shape).collect();
					tileset.set_tile_collision_shapes(tile_id, &shape_list);
				}
			}
		}
		tileset
	}

	//==============================================================================
	/// 물체 하나 읽기.
	//==============================================================================
	fn read_object(object_entry: &Value) -> TiledObject {
		let mut tiled_object = TiledObject::new();
		tiled_object.id = TiledMapReader::read_number(object_entry, "id", 0.0) as u32;
		tiled_object.name = String::from(TiledMapReader::read_string(object_entry, "name", ""));
		let mut object_type = TiledMapReader::read_string(object_entry, "type", "");
		if object_type.is_empty() {
			object_type = TiledMapReader::read_string(object_entry, "class", "");
		}
		tiled_object.object_type = String::from(object_type);
		tiled_object.tile = TiledMapReader::read_number(object_entry, "gid", 0.0) as u32;
		tiled_object.is_visible = object_entry.get("visible").and_then(Value::as_bool) != Some(false);
		tiled_object.properties = TiledMapReader::read_properties(object_entry);
		let is_flag = |key: &str| object_entry.get(key).and_then(Value::as_bool) == Some(true);
		let polygon_list = object_entry.get("polygon").and_then(Value::as_array);
		let polyline_list = object_entry.get("polyline").and_then(Value::as_array);
		let kind = if is_flag("ellipse") {
			TileCollisionShapeKind::Ellipse
		}
		else if is_flag("capsule") {
			TileCollisionShapeKind::Capsule
		}
		else if is_flag("point") {
			TileCollisionShapeKind::Point
		}
		else if polygon_list.is_some() {
			TileCollisionShapeKind::Polygon
		}
		else if polyline_list.is_some() {
			TileCollisionShapeKind::Polyline
		}
		else {
			TileCollisionShapeKind::Rectangle
		};
		let mut shape = TileCollisionShape::new(kind);
		shape.x = TiledMapReader::read_number(object_entry, "x", 0.0) as f32;
		shape.y = TiledMapReader::read_number(object_entry, "y", 0.0) as f32;
		shape.width = TiledMapReader::read_number(object_entry, "width", 0.0) as f32;
		shape.height = TiledMapReader::read_number(object_entry, "height", 0.0) as f32;
		shape.rotation = TiledMapReader::read_number(object_entry, "rotation", 0.0) as f32;
		let point_entry_list = match kind {
			TileCollisionShapeKind::Polygon => {
				polygon_list
			}
			TileCollisionShapeKind::Polyline => {
				polyline_list
			}
			_ => {
				None
			}
		};
		if let Some(point_entry_list) = point_entry_list {
			for point_entry in point_entry_list.iter() {
				shape.points.push(Vector2::create(TiledMapReader::read_number(point_entry, "x", 0.0) as f32, TiledMapReader::read_number(point_entry, "y", 0.0) as f32));
			}
		}
		tiled_object.shape = shape;
		tiled_object
	}

	//==============================================================================
	/// 타일 층 자료 읽기. (배열이나 base64 와 압축을 타일 값 목록으로)
	///
	/// # 인자
	/// - `data`: 층의 data. (gid 배열이나 base64 글)
	/// - `encoding`: "csv" 나 "base64".
	/// - `compression`: 압축. ("", "zlib", "gzip", "zstd" 는 범위 밖)
	/// - `tile_count`: 있어야 할 칸 수. (모자라면 0 으로 채운다)
	///
	/// # 돌려줌
	/// 타일 값. (행 차례) base64 가 틀리거나 압축을 풀지 못하면 Err.
	//==============================================================================
	pub fn read_layer_data(data: &Value, encoding: &str, compression: &str, tile_count: usize) -> Result<Vec<u32>, String> {
		let mut tile_list = vec![0u32; tile_count];
		if let Some(value_list) = data.as_array() {
			for (tile_index, value) in value_list.iter().take(tile_count).enumerate() {
				tile_list[tile_index] = value.as_f64().map(|number| number as u32).unwrap_or(0);
			}
			return Ok(tile_list);
		}
		let data_text = match data.as_str() {
			Some(data_text) => {
				data_text
			}
			None => {
				return Ok(tile_list);
			}
		};
		if encoding == "base64" {
			let encoded_bytes = TiledMapReader::decode_base64(data_text)?;
			let bytes = TiledMapReader::decompress(&encoded_bytes, compression)?;
			for (tile_index, tile_bytes) in bytes.chunks_exact(4).take(tile_count).enumerate() {
				tile_list[tile_index] = u32::from_le_bytes([tile_bytes[0], tile_bytes[1], tile_bytes[2], tile_bytes[3]]);
			}
			return Ok(tile_list);
		}
		for (tile_index, value_text) in data_text.split(',').take(tile_count).enumerate() {
			tile_list[tile_index] = value_text.trim().parse::<f64>().map(|number| number as u32).unwrap_or(0);
		}
		Ok(tile_list)
	}

	//==============================================================================
	/// base64 풀기. (공백은 건너뛴다)
	///
	/// # 인자
	/// - `text`: base64 글.
	///
	/// # 돌려줌
	/// 바이트. 틀린 글자가 있으면 Err.
	//==============================================================================
	pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
		crate::base::dataurl::decode_base64(text).ok_or_else(|| String::from("TiledMapReader: invalid base64"))
	}

	//==============================================================================
	/// 압축 풀기. (zlib 과 gzip 은 flate2)
	///
	/// # 인자
	/// - `bytes`: 압축된 바이트.
	/// - `compression`: "", "zlib", "gzip". ("zstd" 와 그 밖은 범위 밖)
	///
	/// # 돌려줌
	/// 푼 바이트. (압축이 없으면 그대로) 범위 밖 압축이거나 풀지 못하면 Err.
	//==============================================================================
	pub fn decompress(bytes: &[u8], compression: &str) -> Result<Vec<u8>, String> {
		let mut decompressed_bytes: Vec<u8> = Vec::new();
		let read_result = match compression {
			"" => {
				return Ok(bytes.to_vec());
			}
			"zlib" => {
				flate2::read::ZlibDecoder::new(bytes).read_to_end(&mut decompressed_bytes)
			}
			"gzip" => {
				flate2::read::GzDecoder::new(bytes).read_to_end(&mut decompressed_bytes)
			}
			_ => {
				return Err(format!("TiledMapReader: unsupported compression {}", compression));
			}
		};
		match read_result {
			Ok(_) => {
				Ok(decompressed_bytes)
			}
			Err(error) => {
				Err(format!("TiledMapReader: cannot decompress {} ({})", compression, error))
			}
		}
	}

	//==============================================================================
	/// Tiled 색 글 풀기. ("#RRGGBB" 나 "#AARRGGBB", 알파가 앞이다)
	///
	/// # 인자
	/// - `text`: 색 글. (# 은 없어도 된다)
	///
	/// # 돌려줌
	/// 색. 비었거나 꼴이 틀리면 None.
	//==============================================================================
	pub fn parse_color(text: &str) -> Option<Color> {
		let hex_text = text.strip_prefix('#').unwrap_or(text);
		if (hex_text.len() != 6 && hex_text.len() != 8) || !hex_text.bytes().all(|byte| byte.is_ascii_hexdigit()) {
			return None;
		}
		let read_byte = |start_index: usize| u8::from_str_radix(&hex_text[start_index..start_index + 2], 16).unwrap_or(0) as f32 / 255.0;
		if hex_text.len() == 8 {
			return Some(Color::create(read_byte(2), read_byte(4), read_byte(6), read_byte(0)));
		}
		Some(Color::create(read_byte(0), read_byte(2), read_byte(4), 1.0))
	}

	//==============================================================================
	/// 상대 경로 풀기. (기준 파일이 든 폴더에서, . 과 .. 을 줄인다)
	///
	/// # 인자
	/// - `base_path`: 기준 파일 경로. (빈 글이면 상대 경로 그대로)
	/// - `relative_path`: 풀 경로. (/ 로 시작하거나 "이름:" 꼴(주소, 드라이브)이면 그대로)
	///
	/// # 돌려줌
	/// 푼 경로.
	//==============================================================================
	pub fn resolve_path(base_path: &str, relative_path: &str) -> String {
		let normalized_relative_path = relative_path.replace('\\', "/");
		let has_scheme = match normalized_relative_path.find(':') {
			Some(colon_index) => {
				let scheme = &normalized_relative_path[..colon_index];
				!scheme.is_empty() && scheme.as_bytes()[0].is_ascii_alphabetic() && scheme.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'+' || byte == b'.' || byte == b'-')
			}
			None => {
				false
			}
		};
		if normalized_relative_path.starts_with('/') || has_scheme {
			return normalized_relative_path;
		}
		let normalized_base_path = base_path.replace('\\', "/");
		let directory_path = match normalized_base_path.rfind('/') {
			Some(slash_index) => {
				&normalized_base_path[..slash_index + 1]
			}
			None => {
				""
			}
		};
		let combined_path = format!("{}{}", directory_path, normalized_relative_path);
		let is_rooted = combined_path.starts_with('/');
		let mut segment_list: Vec<&str> = Vec::new();
		for segment in combined_path.split('/') {
			if segment.is_empty() || segment == "." {
				continue;
			}
			if segment == ".." && segment_list.last().is_some_and(|last_segment| *last_segment != "..") {
				segment_list.pop();
				continue;
			}
			segment_list.push(segment);
		}
		let joined_path = segment_list.join("/");
		if is_rooted {
			format!("/{}", joined_path)
		}
		else {
			joined_path
		}
	}

	//==============================================================================
	/// 타일맵이 쓰는 그림 경로 모으기. (타일셋 그림과 타일마다의 그림, 겹치지 않게 처음 나온 차례)
	///
	/// # 인자
	/// - `tilemap`: 읽은 타일맵.
	///
	/// # 돌려줌
	/// 그림 경로.
	//==============================================================================
	pub fn collect_image_paths(tilemap: &Tilemap) -> Vec<String> {
		let mut image_path_list: Vec<String> = Vec::new();
		for tileset in tilemap.get_tilesets().iter() {
			let image_path = tileset.get_image_path();
			if !image_path.is_empty() && !image_path_list.iter().any(|existing_path| existing_path == image_path) {
				image_path_list.push(String::from(image_path));
			}
			for tile_id in tileset.get_defined_tile_ids() {
				let tile_image_path = tileset.get_tile_image_path(tile_id);
				if !tile_image_path.is_empty() && !image_path_list.iter().any(|existing_path| existing_path == tile_image_path) {
					image_path_list.push(String::from(tile_image_path));
				}
			}
		}
		image_path_list
	}

	//==============================================================================
	/// 읽은 그림을 타일셋에 걸기. (그림 경로마다 read_image_handler 에 묻는다)
	///
	/// # 인자
	/// - `tilemap`: 읽은 타일맵.
	/// - `read_image_handler`: 그림 경로를 받아 그 그림을 돌려주는 함수. (없으면 None)
	//==============================================================================
	pub fn attach_images(tilemap: &mut Tilemap, read_image_handler: &dyn Fn(&str) -> Option<Rc<Image>>) {
		for tileset_index in 0..tilemap.get_tilesets().len() {
			let tileset = match tilemap.get_tileset_mut(tileset_index) {
				Some(tileset) => {
					tileset
				}
				None => {
					continue;
				}
			};
			let image_path = String::from(tileset.get_image_path());
			if !image_path.is_empty() {
				if let Some(image) = read_image_handler(&image_path) {
					tileset.set_image(Some(image));
				}
			}
			for tile_id in tileset.get_defined_tile_ids() {
				let tile_image_path = String::from(tileset.get_tile_image_path(tile_id));
				if tile_image_path.is_empty() {
					continue;
				}
				if let Some(tile_image) = read_image_handler(&tile_image_path) {
					tileset.set_tile_image(tile_id, Some(tile_image));
				}
			}
		}
	}

	//==============================================================================
	/// 속성 배열 읽기. ([{ name, type, value }] → 이름 → 값)
	//==============================================================================
	fn read_properties(entry: &Value) -> Map<String, Value> {
		let mut properties = Map::new();
		if let Some(property_entry_list) = entry.get("properties").and_then(Value::as_array) {
			for property_entry in property_entry_list.iter() {
				if let Some(name) = property_entry.get("name").and_then(Value::as_str) {
					properties.insert(String::from(name), property_entry.get("value").cloned().unwrap_or(Value::Null));
				}
			}
		}
		properties
	}

	//==============================================================================
	/// 수 읽기. (없거나 수가 아니면 기본값)
	//==============================================================================
	fn read_number(entry: &Value, key: &str, default_value: f64) -> f64 {
		entry.get(key).and_then(Value::as_f64).unwrap_or(default_value)
	}

	//==============================================================================
	/// 글 읽기. (없거나 글이 아니면 기본값)
	//==============================================================================
	fn read_string<'entry>(entry: &'entry Value, key: &str, default_value: &'entry str) -> &'entry str {
		entry.get(key).and_then(Value::as_str).unwrap_or(default_value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental::tilemap::tilemap::TileFlag;
	use std::path::PathBuf;

	//==============================================================================
	/// 예제 자산 폴더. (원본 저장소의 examples/tilemapshowcase/assets, 없으면 None 으로 시험을 건너뛴다)
	//==============================================================================
	fn find_asset_directory() -> Option<PathBuf> {
		let asset_directory_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../vanilla.js/examples/tilemapshowcase/assets");
		if asset_directory_path.is_dir() {
			Some(asset_directory_path)
		}
		else {
			None
		}
	}

	//==============================================================================
	/// 예제 자산 JSON 읽기.
	//==============================================================================
	fn read_asset_json(asset_directory_path: &std::path::Path, file_name: &str) -> Value {
		let text = std::fs::read_to_string(asset_directory_path.join(file_name)).expect("asset file");
		serde_json::from_str(&text).expect("asset json")
	}

	//==============================================================================
	/// 예제 지도 셋 읽기. (원본 check.mjs 의 readOrthogonal, readIsometric, readHexagonal)
	//==============================================================================
	#[test]
	fn read_example_maps() {
		let asset_directory_path = match find_asset_directory() {
			Some(asset_directory_path) => {
				asset_directory_path
			}
			None => {
				eprintln!("tilemap: vanilla.js 저장소가 옆에 없어 예제 지도 시험을 건너뛴다");
				return;
			}
		};
		let read_tileset = |tileset_path: &str| -> Option<Value> {
			let file_name = tileset_path.rsplit('/').next().unwrap_or(tileset_path);
			Some(read_asset_json(&asset_directory_path, file_name))
		};

		let mut orthogonal_tilemap = Tilemap::new();
		let orthogonal_tiled_map = TiledMapReader::read_map(&read_asset_json(&asset_directory_path, "orthogonal.tmj"), &mut orthogonal_tilemap, "assets/orthogonal.tmj", Some(&read_tileset)).expect("orthogonal map");
		assert_eq!((orthogonal_tilemap.get_layer_count(), orthogonal_tilemap.get_tilesets().len()), (3, 3));
		assert_eq!([0, 1, 2].map(|tileset_index| orthogonal_tilemap.get_tileset_first_gid(tileset_index)), [1, 25, 27]);
		assert_eq!(orthogonal_tilemap.get_layer(0).expect("ground").get_tile(13, 7), 4);
		let decoration_layer = orthogonal_tilemap.get_layer(1).expect("decoration");
		assert_eq!(decoration_layer.get_tile(2, 2), 11 | TileFlag::DIAGONAL_FLIP | TileFlag::HORIZONTAL_FLIP);
		assert_eq!(decoration_layer.get_property("isSolid"), Some(&Value::Bool(true)));
		let highlight_layer = orthogonal_tilemap.get_layer(2).expect("highlight");
		let highlight_color = highlight_layer.get_color();
		assert_eq!((highlight_layer.get_name(), highlight_layer.get_opacity()), ("highlight", 0.6));
		assert_eq!(highlight_color, Color::create(1.0, 128.0 / 255.0, 192.0 / 255.0, 1.0));
		let terrain_tileset = orthogonal_tilemap.get_tileset(0).expect("terrain");
		assert_eq!((terrain_tileset.read_animated_tile_id(3, 200.0), terrain_tileset.get_tile_type(7)), (4, "wall"));
		let props_tileset = orthogonal_tilemap.get_tileset(2).expect("props");
		assert_eq!(props_tileset.get_tile_image_path(0), "assets/chest.png");
		assert_eq!(props_tileset.get_tile_property(0, "gold"), Some(&serde_json::json!(30)));
		assert_eq!(orthogonal_tiled_map.properties["title"], "meadow");
		let object_list: Vec<(String, TileCollisionShapeKind, f32, f32)> = orthogonal_tiled_map.object_layers[0].objects.iter().map(|tiled_object| (tiled_object.name.clone(), tiled_object.shape.kind, tiled_object.shape.x, tiled_object.shape.y)).collect();
		assert_eq!(object_list, vec![(String::from("player"), TileCollisionShapeKind::Point, 40.0, 104.0), (String::from("pond"), TileCollisionShapeKind::Ellipse, 168.0, 80.0)]);
		assert_eq!(TiledMapReader::collect_image_paths(&orthogonal_tilemap), vec!["assets/terrain.png", "assets/trees.png", "assets/chest.png", "assets/sign.png"]);

		let mut isometric_tilemap = Tilemap::new();
		TiledMapReader::read_map(&read_asset_json(&asset_directory_path, "isometric.tmj"), &mut isometric_tilemap, "assets/isometric.tmj", Some(&read_tileset)).expect("isometric map");
		assert_eq!(isometric_tilemap.get_orientation(), TilemapOrientation::Isometric);
		assert_eq!((isometric_tilemap.get_layer(0).expect("ground").get_tile(3, 0), isometric_tilemap.get_layer(0).expect("ground").get_tile(6, 6), isometric_tilemap.get_layer(1).expect("blocks").get_tile(0, 5)), (8, 2, 4));
		assert_eq!(isometric_tilemap.get_tileset(0).expect("isometric").read_animated_tile_id(1, 500.0), 2);

		let mut hexagonal_tilemap = Tilemap::new();
		TiledMapReader::read_map(&read_asset_json(&asset_directory_path, "hexagonal.tmj"), &mut hexagonal_tilemap, "assets/hexagonal.tmj", Some(&read_tileset)).expect("hexagonal map");
		let hexagonal_layer = hexagonal_tilemap.get_layer(0).expect("ground");
		let hexagonal_bounds = hexagonal_layer.read_cell_bounds();
		assert_eq!((hexagonal_tilemap.get_orientation(), hexagonal_tilemap.get_hex_side_length(), hexagonal_tilemap.get_stagger_axis()), (TilemapOrientation::Hexagonal, 16, TilemapStaggerAxis::Y));
		assert_eq!((hexagonal_layer.get_tile_count(), hexagonal_bounds.size.x, hexagonal_bounds.size.y), (48, 8.0, 6.0));
		assert_eq!(hexagonal_layer.get_tile(4, 1), 6 | TileFlag::DIAGONAL_FLIP | TileFlag::HEXAGONAL_ROTATION_120);
	}

	//==============================================================================
	/// base64 + zlib, gzip 층과 덩이, 묶음 층의 오프셋과 색. (자산 없이)
	//==============================================================================
	#[test]
	fn read_encoded_layers() {
		use std::io::Write;
		let tile_list: [u32; 4] = [1, 0x8000_0002, 3, 0];
		let raw_bytes: Vec<u8> = tile_list.iter().flat_map(|tile| tile.to_le_bytes()).collect();
		let mut zlib_encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
		zlib_encoder.write_all(&raw_bytes).expect("zlib write");
		let zlib_bytes = zlib_encoder.finish().expect("zlib finish");
		let mut gzip_encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
		gzip_encoder.write_all(&raw_bytes).expect("gzip write");
		let gzip_bytes = gzip_encoder.finish().expect("gzip finish");
		let encode_base64 = |bytes: &[u8]| -> String {
			const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
			let mut text = String::new();
			for byte_group in bytes.chunks(3) {
				let value = (byte_group[0] as u32) << 16 | (*byte_group.get(1).unwrap_or(&0) as u32) << 8 | *byte_group.get(2).unwrap_or(&0) as u32;
				for character_index in 0..4 {
					if character_index <= byte_group.len() {
						text.push(ALPHABET[((value >> (18 - character_index * 6)) & 63) as usize] as char);
					}
					else {
						text.push('=');
					}
				}
			}
			text
		};
		let map_data = serde_json::json!({
			"orientation": "orthogonal", "width": 2, "height": 2, "tilewidth": 8, "tileheight": 8, "infinite": true,
			"tilesets": [{ "firstgid": 1, "name": "a", "tilewidth": 8, "tileheight": 8, "columns": 2, "tilecount": 4 }],
			"layers": [
				{ "type": "tilelayer", "name": "zlib", "encoding": "base64", "compression": "zlib", "width": 2, "height": 2, "data": encode_base64(&zlib_bytes) },
				{ "type": "group", "name": "upper", "offsetx": 3, "opacity": 0.5, "tintcolor": "#ff0000", "layers": [
					{ "type": "tilelayer", "name": "gzip", "encoding": "base64", "compression": "gzip", "offsetx": 2, "opacity": 0.5, "chunks": [{ "x": -16, "y": 0, "width": 2, "height": 2, "data": encode_base64(&gzip_bytes) }] }
				] }
			]
		});
		let mut tilemap = Tilemap::new();
		TiledMapReader::read_map(&map_data, &mut tilemap, "", None).expect("encoded map");
		let zlib_layer = tilemap.get_layer(0).expect("zlib");
		assert_eq!([zlib_layer.get_tile(0, 0), zlib_layer.get_tile(1, 0), zlib_layer.get_tile(0, 1), zlib_layer.get_tile(1, 1)], tile_list);
		let gzip_layer = tilemap.get_layer(1).expect("gzip");
		assert_eq!((gzip_layer.get_tile(-15, 0), gzip_layer.get_tile(-16, 1)), (0x8000_0002, 3));
		assert_eq!((gzip_layer.get_offset().x, gzip_layer.get_opacity(), gzip_layer.get_color()), (5.0, 0.25, Color::create(1.0, 0.0, 0.0, 1.0)));
		let zstd_result = TiledMapReader::read_layer_data(&Value::String(String::from("AAAA")), "base64", "zstd", 1);
		assert_eq!(zstd_result, Err(String::from("TiledMapReader: unsupported compression zstd")));
		assert!(TiledMapReader::read_map(&serde_json::json!({ "orientation": "oblique" }), &mut Tilemap::new(), "", None).is_err());
	}

	//==============================================================================
	/// 색 글과 상대 경로. (원본 parseColor, resolvePath)
	//==============================================================================
	#[test]
	fn parse_color_and_resolve_path() {
		assert_eq!(TiledMapReader::parse_color("#80ff0000").map(|color| color.alpha), Some(128.0 / 255.0));
		assert_eq!(TiledMapReader::parse_color("00ff00").map(|color| color.green), Some(1.0));
		assert_eq!(TiledMapReader::parse_color("nope"), None);
		assert_eq!(TiledMapReader::resolve_path("maps/world/level.tmj", "../tiles/a.png"), "maps/tiles/a.png");
		assert_eq!(TiledMapReader::resolve_path("", "./a.tsj"), "a.tsj");
		assert_eq!(TiledMapReader::resolve_path("a/b.tmj", "/abs/c.png"), "/abs/c.png");
		assert_eq!(TiledMapReader::resolve_path("a/b.tmj", "../../c.png"), "../c.png");
	}
}
