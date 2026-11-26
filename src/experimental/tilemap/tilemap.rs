//==============================================================================
//! 타일맵. (원본 src/experimental/tilemap/tilemap.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 노드에 붙는 컴포넌트다. 노드 내용 영역의 왼쪽 위가 지도의 (0, 0) 이고, 층(TilemapLayer)을 넣은 차례로 그린다.
//! - 배치는 정사각(orthogonal), 아이소메트릭(isometric, 마름모), 엇갈림(staggered, 엇갈린 아이소), 육각(hexagonal) 넷이다.
//!   좌표 바꾸기, 그리는 차례, 타일 그림 정렬(칸의 왼쪽 아래), 뒤집기와 돌리기는 Tiled 렌더러(libtiled)와 같은 셈을 쓴다.
//! - 층의 덩이(chunk)마다 화면에 걸치는지 보고 걸치지 않으면 통째로 건너뛴다. 걸친 덩이는 같은 그림을 쓰는 타일을 꼭짓점 버퍼에 이어 적어
//!   그림이 바뀌거나 버퍼가 찰 때만 `Graphic::draw_vertices` 한 번으로 그린다. (유니티 TilemapRenderer 의 Chunk 모드)
//!   Individual 모드는 덩이를 넘어 지도 전체의 칸 차례로 그린다. (큰 타일이 이웃 덩이와 겹칠 때의 차례가 정확하다)
//! - 타일 애니메이션은 tick 이 흘리는 시계 하나로 모든 칸이 같이 돈다. (Tiled 와 같다)
//! - 충돌 모양은 물리에 기대지 않는다. read_cell_collision_shapes 가 칸마다 뒤집기, 돌리기, 오프셋을 건 지도 좌표 모양(TileCollisionShape)을 돌려준다.
//! - 원본과 다른 점: 타일셋과 층을 값으로 가진다. (원본은 객체 참조라 여러 지도가 같은 타일셋을 나눠 쓴다, 러스트는 Tileset 을 clone 한다)
//!   타일셋, 층 번호를 못 찾으면 None 이다. (원본은 -1, null) 애니메이션 시계는 f64 다.
//==============================================================================
// 옮긴 원문: libtiled 의 렌더러 셈에서 옮김, BSD-2-Clause, THIRD_PARTY_LICENSES.md
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, GlTexture, Graphic, FLOATS_PER_VERTEX};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use crate::experimental::tilemap::tilemaplayer::{TilemapChunk, TilemapLayer};
use crate::experimental::tilemap::tileset::{TileCollisionShape, TileFillMode, TileRenderSize, Tileset};
use crate::misc::grid::GridCell;
use std::rc::Rc;

/// 타일 값에서 gid 만 남기는 비트.
const TILE_ID_MASK: u32 = 0x0FFF_FFFF;
/// 타일 값에서 뒤집기와 돌리기 비트.
const TILE_FLAG_BITS: u32 = 0xF000_0000;
/// 쿼드 하나(삼각형 둘, 꼭짓점 여섯)의 수 개수.
const FLOATS_PER_QUAD: usize = 24;

//==============================================================================
/// 타일 값의 뒤집기와 돌리기 비트. (Tiled 전역 타일 번호의 위 4 비트, 언리얼 PaperTileInfo 의 flip_h, flip_v, flip_d)
/// - 정사각과 아이소: 대각 뒤집기(x, y 바꾸기)를 먼저 하고 좌우, 상하 뒤집기를 한다. 시계 방향 90 도는 대각 + 좌우다.
/// - 육각: DIAGONAL_FLIP 이 시계 방향 60 도, HEXAGONAL_ROTATION_120 이 120 도 돌리기다.
///
/// # 예제
/// 비트 합치기
/// ```rust
/// use vanilla::experimental::tilemap::{TileFlag, Tilemap};
///
/// let tile = Tilemap::compose_tile(5, TileFlag::HORIZONTAL_FLIP | TileFlag::VERTICAL_FLIP);
/// assert_eq!(Tilemap::read_tile_id(tile), 5);
/// assert_eq!(tile, Tilemap::rotate_tile(5, 2));
/// ```
//==============================================================================
pub struct TileFlag;

impl TileFlag {
	/// 좌우 뒤집기.
	pub const HORIZONTAL_FLIP: u32 = 0x8000_0000;
	/// 상하 뒤집기.
	pub const VERTICAL_FLIP: u32 = 0x4000_0000;
	/// 대각 뒤집기. (육각은 60 도 돌리기)
	pub const DIAGONAL_FLIP: u32 = 0x2000_0000;
	/// 육각 120 도 돌리기. (그 밖의 배치에서는 쓰지 않는다)
	pub const HEXAGONAL_ROTATION_120: u32 = 0x1000_0000;
}

//==============================================================================
/// 타일맵 배치.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilemapOrientation {
	/// 정사각 격자.
	Orthogonal,
	/// 마름모 아이소메트릭. (지도 전체가 마름모)
	Isometric,
	/// 엇갈린 아이소메트릭. (지도 전체가 네모, 한 줄씩 반 칸 엇갈린다)
	Staggered,
	/// 엇갈린 육각.
	Hexagonal,
}

//==============================================================================
/// 엇갈리는 축. (엇갈림, 육각)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilemapStaggerAxis {
	/// 열이 위아래로 반 칸 엇갈린다. (꼭대기가 평평한 육각)
	X,
	/// 행이 좌우로 반 칸 엇갈린다. (꼭대기가 뾰족한 육각)
	Y,
}

//==============================================================================
/// 엇갈리는 줄. (엇갈림, 육각)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilemapStaggerIndex {
	/// 홀수 줄이 밀린다.
	Odd,
	/// 짝수 줄이 밀린다.
	Even,
}

//==============================================================================
/// 정사각 배치의 그리는 차례. (Tiled renderorder, 유니티 TilemapRenderer.sortOrder)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilemapRenderOrder {
	/// 위 행부터, 행마다 왼쪽에서 오른쪽.
	RightDown,
	/// 아래 행부터, 행마다 왼쪽에서 오른쪽.
	RightUp,
	/// 위 행부터, 행마다 오른쪽에서 왼쪽.
	LeftDown,
	/// 아래 행부터, 행마다 오른쪽에서 왼쪽.
	LeftUp,
}

//==============================================================================
/// 그리는 방식. (유니티 TilemapRenderer.Mode)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TilemapRenderMode {
	/// 덩이마다 묶어 그린다. (빠르다, 덩이 경계를 넘어 겹치는 큰 타일의 차례는 덩이 차례를 따른다)
	Chunk,
	/// 지도 전체의 칸 차례로 그린다. (화면 밖 덩이는 그대로 거른다)
	Individual,
}

//==============================================================================
/// 배치 셈에 쓰는 값. (Tiled HexagonalRenderer::RenderParams 를 함께 셈한다)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct TilemapLayout {
	orientation: TilemapOrientation,
	tile_width: f32,
	tile_height: f32,
	render_order: TilemapRenderOrder,
	origin_x: f32,
	half_tile_width: f32,
	is_stagger_x: bool,
	is_stagger_even: bool,
	side_length_x: f32,
	side_length_y: f32,
	side_offset_x: f32,
	side_offset_y: f32,
	column_width: f32,
	row_height: f32,
	hexagon_tile_width: f32,
	hexagon_tile_height: f32,
}

impl TilemapLayout {
	//==============================================================================
	/// 엇갈린 줄인지. (Tiled doStaggerX, doStaggerY)
	//==============================================================================
	fn is_staggered_index(&self, index: i32) -> bool {
		let is_odd = (index & 1) != 0;
		is_odd != self.is_stagger_even
	}

	//==============================================================================
	/// 엇갈림, 육각 배치인지.
	//==============================================================================
	fn is_hexagonal_family(&self) -> bool {
		self.orientation == TilemapOrientation::Staggered || self.orientation == TilemapOrientation::Hexagonal
	}

	//==============================================================================
	/// 칸을 감싸는 사각형의 왼쪽 위 X. (지도 좌표)
	//==============================================================================
	fn read_cell_left(&self, column: i32, row: i32) -> f32 {
		if self.orientation == TilemapOrientation::Isometric {
			return (column - row) as f32 * self.tile_width / 2.0 + self.origin_x - self.half_tile_width;
		}
		if self.is_hexagonal_family() {
			if self.is_stagger_x {
				return column as f32 * self.column_width;
			}
			let mut left = column as f32 * (self.hexagon_tile_width + self.side_length_x);
			if self.is_staggered_index(row) {
				left += self.column_width;
			}
			return left;
		}
		column as f32 * self.tile_width
	}

	//==============================================================================
	/// 칸을 감싸는 사각형의 왼쪽 위 Y. (지도 좌표)
	//==============================================================================
	fn read_cell_top(&self, column: i32, row: i32) -> f32 {
		if self.orientation == TilemapOrientation::Isometric {
			return (column + row) as f32 * self.tile_height / 2.0;
		}
		if self.is_hexagonal_family() {
			if !self.is_stagger_x {
				return row as f32 * self.row_height;
			}
			let mut top = row as f32 * (self.hexagon_tile_height + self.side_length_y);
			if self.is_staggered_index(column) {
				top += self.row_height;
			}
			return top;
		}
		row as f32 * self.tile_height
	}

	//==============================================================================
	/// 칸을 감싸는 사각형의 너비.
	//==============================================================================
	fn read_cell_width(&self) -> f32 {
		if self.is_hexagonal_family() {
			return self.hexagon_tile_width;
		}
		self.tile_width
	}

	//==============================================================================
	/// 칸을 감싸는 사각형의 높이.
	//==============================================================================
	fn read_cell_height(&self) -> f32 {
		if self.is_hexagonal_family() {
			return self.hexagon_tile_height;
		}
		self.tile_height
	}

	//==============================================================================
	/// 칸 범위를 감싸는 사각형 셈하기. (Tiled MapRenderer::boundingRect, 지도 좌표)
	//==============================================================================
	fn read_cell_range_bounds(&self, column: i32, row: i32, column_count: i32, row_count: i32) -> Rect {
		if self.orientation == TilemapOrientation::Isometric {
			let side = (column_count + row_count) as f32;
			let left = (column - (row + row_count)) as f32 * self.tile_width / 2.0 + self.origin_x;
			let top = (column + row) as f32 * self.tile_height / 2.0;
			return Rect::create(left, top, side * self.tile_width / 2.0, side * self.tile_height / 2.0);
		}
		if self.is_hexagonal_family() {
			let mut left = self.read_cell_left(column, row);
			let mut top = self.read_cell_top(column, row);
			let width;
			let mut height;
			if self.is_stagger_x {
				width = column_count as f32 * self.column_width + self.side_offset_x;
				height = row_count as f32 * (self.hexagon_tile_height + self.side_length_y);
				if column_count > 1 {
					height += self.row_height;
					if self.is_staggered_index(column) {
						top -= self.row_height;
					}
				}
				return Rect::create(left, top, width, height);
			}
			let mut width_value = column_count as f32 * (self.hexagon_tile_width + self.side_length_x);
			height = row_count as f32 * self.row_height + self.side_offset_y;
			if row_count > 1 {
				width_value += self.column_width;
				if self.is_staggered_index(row) {
					left -= self.column_width;
				}
			}
			return Rect::create(left, top, width_value, height);
		}
		Rect::create(column as f32 * self.tile_width, row as f32 * self.tile_height, column_count as f32 * self.tile_width, row_count as f32 * self.tile_height)
	}

	//==============================================================================
	/// 칸 범위를 그리는 차례로 돌기. (Tiled 렌더러의 drawTileLayer 차례)
	/// - 정사각: 그리는 차례(renderorder)대로. 아이소: 화면 줄(열 + 행)마다 왼쪽에서 오른쪽.
	/// - 엇갈림, 육각: 행마다. X 축으로 엇갈리면 그 행의 밀리지 않은 열을 먼저, 밀린 열을 나중에 그린다.
	//==============================================================================
	fn visit_cells_in_draw_order(&self, column: i32, row: i32, column_count: i32, row_count: i32, visit_handler: &mut dyn FnMut(i32, i32)) {
		let last_column = column + column_count - 1;
		let last_row = row + row_count - 1;
		if self.orientation == TilemapOrientation::Isometric {
			for diagonal in (column + row)..=(last_column + last_row) {
				let first_diagonal_column = column.max(diagonal - last_row);
				let last_diagonal_column = last_column.min(diagonal - row);
				for diagonal_column in first_diagonal_column..=last_diagonal_column {
					visit_handler(diagonal_column, diagonal - diagonal_column);
				}
			}
			return;
		}
		if self.is_hexagonal_family() {
			for visit_row in row..=last_row {
				if !self.is_stagger_x {
					for visit_column in column..=last_column {
						visit_handler(visit_column, visit_row);
					}
					continue;
				}
				for visit_column in column..=last_column {
					if !self.is_staggered_index(visit_column) {
						visit_handler(visit_column, visit_row);
					}
				}
				for visit_column in column..=last_column {
					if self.is_staggered_index(visit_column) {
						visit_handler(visit_column, visit_row);
					}
				}
			}
			return;
		}
		let is_up = self.render_order == TilemapRenderOrder::RightUp || self.render_order == TilemapRenderOrder::LeftUp;
		let is_left = self.render_order == TilemapRenderOrder::LeftDown || self.render_order == TilemapRenderOrder::LeftUp;
		for row_step in 0..row_count {
			let visit_row = if is_up {
				last_row - row_step
			}
			else {
				row + row_step
			};
			for column_step in 0..column_count {
				let visit_column = if is_left {
					last_column - column_step
				}
				else {
					column + column_step
				};
				visit_handler(visit_column, visit_row);
			}
		}
	}

	//==============================================================================
	/// 덩이를 그리는 차례로 늘어놓기. (덩이 단위의 열, 행을 칸처럼 본다)
	//==============================================================================
	fn sort_chunks_in_draw_order(&self, chunk_list: &mut [&TilemapChunk]) {
		if self.orientation == TilemapOrientation::Isometric {
			chunk_list.sort_by_key(|chunk| (chunk.column + chunk.row, chunk.column));
			return;
		}
		if self.orientation == TilemapOrientation::Orthogonal {
			let row_sign = if self.render_order == TilemapRenderOrder::RightUp || self.render_order == TilemapRenderOrder::LeftUp {
				-1
			}
			else {
				1
			};
			let column_sign = if self.render_order == TilemapRenderOrder::LeftDown || self.render_order == TilemapRenderOrder::LeftUp {
				-1
			}
			else {
				1
			};
			chunk_list.sort_by_key(|chunk| (chunk.row * row_sign, chunk.column * column_sign));
			return;
		}
		chunk_list.sort_by_key(|chunk| (chunk.row, chunk.column));
	}
}

//==============================================================================
/// 칸 하나를 그리는 자리. (Tiled CellRenderer::render 의 셈)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct TilePlacement {
	source_x: f32,
	source_y: f32,
	source_width: f32,
	source_height: f32,
	scale_x: f32,
	scale_y: f32,
	center_x: f32,
	center_y: f32,
	is_horizontal_flip: bool,
	is_vertical_flip: bool,
	rotation: f32,
}

impl TilePlacement {
	//==============================================================================
	/// 쿼드 꼭짓점 셈하기. (그림 영역의 네 모서리를 가운데 기준으로 뒤집고 돌려 놓는다)
	/// - 차례: 왼쪽 위, 오른쪽 위, 왼쪽 아래, 왼쪽 아래, 오른쪽 위, 오른쪽 아래. 꼭짓점마다 x, y, u, v.
	//==============================================================================
	fn write_quad(&self, image_width: f32, image_height: f32) -> [f32; FLOATS_PER_QUAD] {
		let horizontal_sign = if self.is_horizontal_flip {
			-1.0
		}
		else {
			1.0
		};
		let vertical_sign = if self.is_vertical_flip {
			-1.0
		}
		else {
			1.0
		};
		let half_width = (self.source_width * self.scale_x * 0.5 * horizontal_sign) as f64;
		let half_height = (self.source_height * self.scale_y * 0.5 * vertical_sign) as f64;
		let mut cos_rotation = 1.0f64;
		let mut sin_rotation = 0.0f64;
		if self.rotation != 0.0 {
			let radian = (self.rotation as f64).to_radians();
			cos_rotation = radian.cos();
			sin_rotation = radian.sin();
		}
		let center_x = self.center_x as f64;
		let center_y = self.center_y as f64;
		let left_top_x = (-half_width * cos_rotation + half_height * sin_rotation + center_x) as f32;
		let left_top_y = (-half_width * sin_rotation - half_height * cos_rotation + center_y) as f32;
		let right_top_x = (half_width * cos_rotation + half_height * sin_rotation + center_x) as f32;
		let right_top_y = (half_width * sin_rotation - half_height * cos_rotation + center_y) as f32;
		let left_bottom_x = (-half_width * cos_rotation - half_height * sin_rotation + center_x) as f32;
		let left_bottom_y = (-half_width * sin_rotation + half_height * cos_rotation + center_y) as f32;
		let right_bottom_x = (half_width * cos_rotation - half_height * sin_rotation + center_x) as f32;
		let right_bottom_y = (half_width * sin_rotation + half_height * cos_rotation + center_y) as f32;
		let left_u = self.source_x / image_width;
		let top_v = self.source_y / image_height;
		let right_u = (self.source_x + self.source_width) / image_width;
		let bottom_v = (self.source_y + self.source_height) / image_height;
		[
			left_top_x, left_top_y, left_u, top_v,
			right_top_x, right_top_y, right_u, top_v,
			left_bottom_x, left_bottom_y, left_u, bottom_v,
			left_bottom_x, left_bottom_y, left_u, bottom_v,
			right_top_x, right_top_y, right_u, top_v,
			right_bottom_x, right_bottom_y, right_u, bottom_v,
		]
	}
}

//==============================================================================
/// 그리는 동안의 상태. (꼭짓점 버퍼에 쌓인 수, 지금 그림과 텍스처, 지난 타일셋 범위)
//==============================================================================
struct TilemapDrawState {
	float_capacity: usize,
	float_offset: usize,
	image: Option<Rc<Image>>,
	texture: Option<GlTexture>,
	color: Color,
	time_milliseconds: f64,
	tileset_index: usize,
	tileset_first_gid: u32,
	tileset_last_gid: u32,
}

//==============================================================================
/// 타일맵 컴포넌트.
///
/// # 예제
/// 좌표 바꾸기와 칸의 충돌 모양
/// ```rust
/// use vanilla::experimental::tilemap::{TileCollisionShape, Tilemap, TilemapOrientation, Tileset};
///
/// let mut tilemap = Tilemap::new();
/// tilemap.set_orientation(TilemapOrientation::Isometric);
/// tilemap.set_tile_size(64, 32);
/// tilemap.set_map_size(10, 10);
/// let center = tilemap.cell_to_map_center(2, 3);
/// let cell = tilemap.map_position_to_cell(center.x, center.y);
/// assert_eq!((center.x, center.y, cell.column, cell.row), (288.0, 96.0, 2, 3));
///
/// let mut tileset = Tileset::new();
/// tileset.set_tile_size(64, 32);
/// tileset.set_columns(4);
/// tileset.set_tile_count(16);
/// tileset.set_tile_collision_shapes(0, &[TileCollisionShape::create_rectangle(0.0, 0.0, 64.0, 32.0)]);
/// tilemap.add_tileset(tileset, 0);
/// tilemap.create_layer("walls").set_tile(2, 3, 1);
/// let shape_list = tilemap.read_cell_collision_shapes(0, 2, 3);
/// assert_eq!((shape_list.len(), shape_list[0].x, shape_list[0].y), (1, 256.0, 80.0));
/// ```
///
/// 노드에 붙여 그리기
/// ```rust,no_run
/// use vanilla::experimental::tilemap::{Tilemap, Tileset};
/// use vanilla::{Engine, EngineConfiguration, Image, Pivot, Scene, SceneBase, Vector2};
///
/// struct MainScene {
/// 	scene_base: SceneBase,
/// }
///
/// impl Scene for MainScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn initialize(&mut self, _engine: &mut Engine) {
/// 		let image = Image::load(std::path::Path::new("terrain.png")).expect("terrain image");
/// 		let mut tilemap = Tilemap::new();
/// 		tilemap.set_tile_size(16, 16);
/// 		tilemap.add_tileset(Tileset::create_grid(image, 16, 16, 1, 2), 0);
/// 		let layer = tilemap.create_layer("ground");
/// 		for row in 0..6 {
/// 			for column in 0..10 {
/// 				layer.set_tile(column, row, 1);
/// 			}
/// 		}
/// 		layer.set_tile(4, 2, Tilemap::rotate_tile(3, 1));
/// 		let root_id = self.scene_base.get_root_id();
/// 		let node_tree = self.scene_base.get_node_tree_mut();
/// 		let map_id = node_tree.create_child(root_id, "map");
/// 		let map_node = &mut node_tree[map_id];
/// 		map_node.set_pivot(Pivot::TOP_LEFT);
/// 		map_node.set_local_position(Vector2::create(160.0, 100.0));
/// 		map_node.set_local_scale(Vector2::create(2.0, 2.0));
/// 		map_node.add_component(tilemap);
/// 	}
/// }
///
/// let engine = Engine::new(EngineConfiguration::new());
/// engine.run(Box::new(MainScene { scene_base: SceneBase::new() })).expect("run");
/// ```
//==============================================================================
pub struct Tilemap {
	component_base: ComponentBase,
	orientation: TilemapOrientation,
	tile_width: u32,
	tile_height: u32,
	hex_side_length: u32,
	stagger_axis: TilemapStaggerAxis,
	stagger_index: TilemapStaggerIndex,
	render_order: TilemapRenderOrder,
	render_mode: TilemapRenderMode,
	column_count: u32,
	row_count: u32,
	tilesets: Vec<Tileset>,
	tileset_first_gids: Vec<u32>,
	layers: Vec<TilemapLayer>,
	animation_time: f64,
	animation_speed: f32,
	last_drawn_chunk_count: u32,
	last_culled_chunk_count: u32,
}

impl Tilemap {
	//==============================================================================
	/// 생성. (정사각, 칸 32 x 32)
	///
	/// # 돌려줌
	/// 빈 타일맵.
	//==============================================================================
	pub fn new() -> Tilemap {
		Tilemap {
			component_base: ComponentBase::new(),
			orientation: TilemapOrientation::Orthogonal,
			tile_width: 32,
			tile_height: 32,
			hex_side_length: 0,
			stagger_axis: TilemapStaggerAxis::Y,
			stagger_index: TilemapStaggerIndex::Odd,
			render_order: TilemapRenderOrder::RightDown,
			render_mode: TilemapRenderMode::Chunk,
			column_count: 0,
			row_count: 0,
			tilesets: Vec::new(),
			tileset_first_gids: Vec::new(),
			layers: Vec::new(),
			animation_time: 0.0,
			animation_speed: 1.0,
			last_drawn_chunk_count: 0,
			last_culled_chunk_count: 0,
		}
	}

	//==============================================================================
	/// 타일 값에서 gid 읽기. (뒤집기 비트를 지운다)
	///
	/// # 인자
	/// - `tile`: 타일 값.
	///
	/// # 돌려줌
	/// gid. (0 은 빈 칸)
	//==============================================================================
	pub fn read_tile_id(tile: u32) -> u32 {
		tile & TILE_ID_MASK
	}

	//==============================================================================
	/// 타일 값에서 뒤집기와 돌리기 비트 읽기.
	///
	/// # 인자
	/// - `tile`: 타일 값.
	///
	/// # 돌려줌
	/// TileFlag 비트를 합친 값.
	//==============================================================================
	pub fn read_tile_flags(tile: u32) -> u32 {
		tile & TILE_FLAG_BITS
	}

	//==============================================================================
	/// gid 와 뒤집기 비트로 타일 값 짓기.
	///
	/// # 인자
	/// - `tile_id`: gid.
	/// - `tile_flags`: TileFlag 비트를 합친 값.
	///
	/// # 돌려줌
	/// 타일 값.
	//==============================================================================
	pub fn compose_tile(tile_id: u32, tile_flags: u32) -> u32 {
		(tile_id & TILE_ID_MASK) | (tile_flags & TILE_FLAG_BITS)
	}

	//==============================================================================
	/// 타일 값의 방향에 2 x 2 변환을 더 걸기. (정사각과 아이소의 대각, 좌우, 상하 비트로 다시 나눈다)
	/// - 비트의 방향 T = 상하^v x 좌우^h x 대각^d (대각을 먼저 건다) 를 행렬로 만들어 앞에 곱하고 다시 비트로 나눈다.
	//==============================================================================
	fn transform_tile(tile: u32, matrix_a: i32, matrix_b: i32, matrix_c: i32, matrix_d: i32) -> u32 {
		let tile_flags = Tilemap::read_tile_flags(tile);
		let is_diagonal = (tile_flags & TileFlag::DIAGONAL_FLIP) != 0;
		let is_horizontal = (tile_flags & TileFlag::HORIZONTAL_FLIP) != 0;
		let is_vertical = (tile_flags & TileFlag::VERTICAL_FLIP) != 0;
		// T 의 행렬. (열 벡터 [x, y] 에 곱한다)
		let mut tile_a = 1;
		let mut tile_b = 0;
		let mut tile_c = 0;
		let mut tile_d = 1;
		if is_diagonal {
			tile_a = 0;
			tile_b = 1;
			tile_c = 1;
			tile_d = 0;
		}
		if is_horizontal {
			tile_a = -tile_a;
			tile_c = -tile_c;
		}
		if is_vertical {
			tile_b = -tile_b;
			tile_d = -tile_d;
		}
		// 앞에 곱하기. (새 방향 = 건 변환 x T, 행렬 [a c; b d])
		let result_a = matrix_a * tile_a + matrix_c * tile_b;
		let result_b = matrix_b * tile_a + matrix_d * tile_b;
		let result_c = matrix_a * tile_c + matrix_c * tile_d;
		let result_d = matrix_b * tile_c + matrix_d * tile_d;
		let mut result_flags = tile_flags & TileFlag::HEXAGONAL_ROTATION_120;
		if result_a == 0 {
			result_flags |= TileFlag::DIAGONAL_FLIP;
			if result_c < 0 {
				result_flags |= TileFlag::HORIZONTAL_FLIP;
			}
			if result_b < 0 {
				result_flags |= TileFlag::VERTICAL_FLIP;
			}
		}
		else {
			if result_a < 0 {
				result_flags |= TileFlag::HORIZONTAL_FLIP;
			}
			if result_d < 0 {
				result_flags |= TileFlag::VERTICAL_FLIP;
			}
		}
		Tilemap::compose_tile(Tilemap::read_tile_id(tile), result_flags)
	}

	//==============================================================================
	/// 타일 값을 시계 방향으로 90 도씩 돌리기. (정사각과 아이소)
	///
	/// # 인자
	/// - `tile`: 타일 값. (이미 건 뒤집기 위에 더 돌린다)
	/// - `clockwise_quarter_turn_count`: 시계 방향 90 도 횟수. (음수면 반시계)
	///
	/// # 돌려줌
	/// 돌린 타일 값.
	//==============================================================================
	pub fn rotate_tile(tile: u32, clockwise_quarter_turn_count: i32) -> u32 {
		let turn_count = clockwise_quarter_turn_count.rem_euclid(4);
		let mut rotated_tile = tile;
		for _ in 0..turn_count {
			rotated_tile = Tilemap::transform_tile(rotated_tile, 0, 1, -1, 0);
		}
		rotated_tile
	}

	//==============================================================================
	/// 타일 값을 좌우로 뒤집기. (이미 건 방향 위에 화면 기준으로 뒤집는다)
	///
	/// # 인자
	/// - `tile`: 타일 값.
	///
	/// # 돌려줌
	/// 뒤집은 타일 값.
	//==============================================================================
	pub fn flip_tile_horizontally(tile: u32) -> u32 {
		Tilemap::transform_tile(tile, -1, 0, 0, 1)
	}

	//==============================================================================
	/// 타일 값을 상하로 뒤집기. (이미 건 방향 위에 화면 기준으로 뒤집는다)
	///
	/// # 인자
	/// - `tile`: 타일 값.
	///
	/// # 돌려줌
	/// 뒤집은 타일 값.
	//==============================================================================
	pub fn flip_tile_vertically(tile: u32) -> u32 {
		Tilemap::transform_tile(tile, 1, 0, 0, -1)
	}

	//==============================================================================
	/// 배치 셈에 쓰는 값 모으기.
	//==============================================================================
	fn create_layout(&self) -> TilemapLayout {
		let tile_width = self.tile_width as i32;
		let tile_height = self.tile_height as i32;
		let hex_side_length = self.hex_side_length as i32;
		let is_stagger_x = self.stagger_axis == TilemapStaggerAxis::X;
		let mut side_length_x = 0;
		let mut side_length_y = 0;
		if self.orientation == TilemapOrientation::Hexagonal {
			if is_stagger_x {
				side_length_x = hex_side_length;
			}
			else {
				side_length_y = hex_side_length;
			}
		}
		let side_offset_x = (tile_width - side_length_x) / 2;
		let side_offset_y = (tile_height - side_length_y) / 2;
		let column_width = side_offset_x + side_length_x;
		let row_height = side_offset_y + side_length_y;
		TilemapLayout {
			orientation: self.orientation,
			tile_width: tile_width as f32,
			tile_height: tile_height as f32,
			render_order: self.render_order,
			origin_x: ((self.row_count as i64 * tile_width as i64) / 2) as f32,
			half_tile_width: (tile_width / 2) as f32,
			is_stagger_x,
			is_stagger_even: self.stagger_index == TilemapStaggerIndex::Even,
			side_length_x: side_length_x as f32,
			side_length_y: side_length_y as f32,
			side_offset_x: side_offset_x as f32,
			side_offset_y: side_offset_y as f32,
			column_width: column_width as f32,
			row_height: row_height as f32,
			hexagon_tile_width: (column_width + side_offset_x) as f32,
			hexagon_tile_height: (row_height + side_offset_y) as f32,
		}
	}

	//==============================================================================
	/// 배치 설정.
	///
	/// # 인자
	/// - `orientation`: 배치.
	//==============================================================================
	pub fn set_orientation(&mut self, orientation: TilemapOrientation) {
		self.orientation = orientation;
	}

	//==============================================================================
	/// 배치 반환.
	///
	/// # 돌려줌
	/// 배치. (처음은 Orthogonal)
	//==============================================================================
	pub fn get_orientation(&self) -> TilemapOrientation {
		self.orientation
	}

	//==============================================================================
	/// 격자 칸 크기 설정.
	///
	/// # 인자
	/// - `tile_width`: 칸 너비. (픽셀, 아이소는 마름모 너비)
	/// - `tile_height`: 칸 높이. (픽셀, 아이소는 마름모 높이)
	//==============================================================================
	pub fn set_tile_size(&mut self, tile_width: u32, tile_height: u32) {
		self.tile_width = tile_width;
		self.tile_height = tile_height;
	}

	//==============================================================================
	/// 격자 칸 너비 반환.
	///
	/// # 돌려줌
	/// 칸 너비. (픽셀, 처음은 32)
	//==============================================================================
	pub fn get_tile_width(&self) -> u32 {
		self.tile_width
	}

	//==============================================================================
	/// 격자 칸 높이 반환.
	///
	/// # 돌려줌
	/// 칸 높이. (픽셀, 처음은 32)
	//==============================================================================
	pub fn get_tile_height(&self) -> u32 {
		self.tile_height
	}

	//==============================================================================
	/// 육각의 곧은 변 길이 설정. (육각 배치)
	///
	/// # 인자
	/// - `hex_side_length`: 곧은 변 길이. (픽셀, Y 축으로 엇갈리면 세로 변, X 축이면 가로 변)
	//==============================================================================
	pub fn set_hex_side_length(&mut self, hex_side_length: u32) {
		self.hex_side_length = hex_side_length;
	}

	//==============================================================================
	/// 육각의 곧은 변 길이 반환.
	///
	/// # 돌려줌
	/// 곧은 변 길이. (픽셀, 처음은 0)
	//==============================================================================
	pub fn get_hex_side_length(&self) -> u32 {
		self.hex_side_length
	}

	//==============================================================================
	/// 엇갈리는 축 설정. (엇갈림, 육각)
	///
	/// # 인자
	/// - `stagger_axis`: 축.
	//==============================================================================
	pub fn set_stagger_axis(&mut self, stagger_axis: TilemapStaggerAxis) {
		self.stagger_axis = stagger_axis;
	}

	//==============================================================================
	/// 엇갈리는 축 반환.
	///
	/// # 돌려줌
	/// 축. (처음은 Y)
	//==============================================================================
	pub fn get_stagger_axis(&self) -> TilemapStaggerAxis {
		self.stagger_axis
	}

	//==============================================================================
	/// 엇갈리는 줄 설정. (엇갈림, 육각)
	///
	/// # 인자
	/// - `stagger_index`: 줄.
	//==============================================================================
	pub fn set_stagger_index(&mut self, stagger_index: TilemapStaggerIndex) {
		self.stagger_index = stagger_index;
	}

	//==============================================================================
	/// 엇갈리는 줄 반환.
	///
	/// # 돌려줌
	/// 줄. (처음은 Odd)
	//==============================================================================
	pub fn get_stagger_index(&self) -> TilemapStaggerIndex {
		self.stagger_index
	}

	//==============================================================================
	/// 정사각의 그리는 차례 설정.
	///
	/// # 인자
	/// - `render_order`: 차례.
	//==============================================================================
	pub fn set_render_order(&mut self, render_order: TilemapRenderOrder) {
		self.render_order = render_order;
	}

	//==============================================================================
	/// 정사각의 그리는 차례 반환.
	///
	/// # 돌려줌
	/// 차례. (처음은 RightDown)
	//==============================================================================
	pub fn get_render_order(&self) -> TilemapRenderOrder {
		self.render_order
	}

	//==============================================================================
	/// 그리는 방식 설정.
	///
	/// # 인자
	/// - `render_mode`: 방식.
	//==============================================================================
	pub fn set_render_mode(&mut self, render_mode: TilemapRenderMode) {
		self.render_mode = render_mode;
	}

	//==============================================================================
	/// 그리는 방식 반환.
	///
	/// # 돌려줌
	/// 방식. (처음은 Chunk)
	//==============================================================================
	pub fn get_render_mode(&self) -> TilemapRenderMode {
		self.render_mode
	}

	//==============================================================================
	/// 지도 크기 설정. (칸 수, 아이소의 원점과 read_map_pixel_size 에 쓴다. 칸은 이 밖에도 쓸 수 있다)
	///
	/// # 인자
	/// - `column_count`: 열 수.
	/// - `row_count`: 행 수.
	//==============================================================================
	pub fn set_map_size(&mut self, column_count: u32, row_count: u32) {
		self.column_count = column_count;
		self.row_count = row_count;
	}

	//==============================================================================
	/// 지도 열 수 반환.
	///
	/// # 돌려줌
	/// 열 수. (처음은 0)
	//==============================================================================
	pub fn get_column_count(&self) -> u32 {
		self.column_count
	}

	//==============================================================================
	/// 지도 행 수 반환.
	///
	/// # 돌려줌
	/// 행 수. (처음은 0)
	//==============================================================================
	pub fn get_row_count(&self) -> u32 {
		self.row_count
	}

	//==============================================================================
	/// 타일셋 넣기.
	///
	/// # 인자
	/// - `tileset`: 넣을 타일셋.
	/// - `first_gid`: 이 타일셋 0 번 타일의 gid. (0 이면 앞 타일셋 다음 번호, 첫 타일셋은 1)
	///
	/// # 돌려줌
	/// 정한 첫 gid.
	//==============================================================================
	pub fn add_tileset(&mut self, tileset: Tileset, first_gid: u32) -> u32 {
		let mut resolved_first_gid = first_gid;
		if resolved_first_gid == 0 {
			resolved_first_gid = 1;
			if let (Some(last_tileset), Some(last_first_gid)) = (self.tilesets.last(), self.tileset_first_gids.last()) {
				let mut last_id_count = last_tileset.get_tile_count();
				if let Some(last_defined_tile_id) = last_tileset.get_defined_tile_ids().last() {
					last_id_count = last_id_count.max(last_defined_tile_id + 1);
				}
				resolved_first_gid = last_first_gid + last_id_count.max(1);
			}
		}
		// 첫 gid 차례를 지켜 끼운다.
		let mut insert_index = self.tilesets.len();
		while insert_index > 0 && self.tileset_first_gids[insert_index - 1] > resolved_first_gid {
			insert_index -= 1;
		}
		self.tilesets.insert(insert_index, tileset);
		self.tileset_first_gids.insert(insert_index, resolved_first_gid);
		resolved_first_gid
	}

	//==============================================================================
	/// 타일셋 목록 반환.
	///
	/// # 돌려줌
	/// 타일셋. (첫 gid 차례)
	//==============================================================================
	pub fn get_tilesets(&self) -> &[Tileset] {
		&self.tilesets
	}

	//==============================================================================
	/// 타일셋 반환.
	///
	/// # 인자
	/// - `tileset_index`: 타일셋 번호.
	///
	/// # 돌려줌
	/// 타일셋. (없으면 None)
	//==============================================================================
	pub fn get_tileset(&self, tileset_index: usize) -> Option<&Tileset> {
		self.tilesets.get(tileset_index)
	}

	//==============================================================================
	/// 타일셋 반환. (변경용)
	///
	/// # 인자
	/// - `tileset_index`: 타일셋 번호.
	///
	/// # 돌려줌
	/// 타일셋. (없으면 None)
	//==============================================================================
	pub fn get_tileset_mut(&mut self, tileset_index: usize) -> Option<&mut Tileset> {
		self.tilesets.get_mut(tileset_index)
	}

	//==============================================================================
	/// 타일셋의 첫 gid 반환.
	///
	/// # 인자
	/// - `tileset_index`: 타일셋 번호.
	///
	/// # 돌려줌
	/// 첫 gid. (없으면 0)
	//==============================================================================
	pub fn get_tileset_first_gid(&self, tileset_index: usize) -> u32 {
		self.tileset_first_gids.get(tileset_index).cloned().unwrap_or(0)
	}

	//==============================================================================
	/// gid 가 든 타일셋 번호 찾기. (첫 gid 가 gid 이하인 것 가운데 가장 큰 것, Tiled 와 같다)
	///
	/// # 인자
	/// - `tile_id`: gid. (뒤집기 비트가 있으면 지우고 본다)
	///
	/// # 돌려줌
	/// 타일셋 번호. (없거나 빈 칸이면 None, 타일셋 안의 번호는 gid - get_tileset_first_gid)
	//==============================================================================
	pub fn find_tileset_index(&self, tile_id: u32) -> Option<usize> {
		let gid = Tilemap::read_tile_id(tile_id);
		if gid == 0 {
			return None;
		}
		(0..self.tileset_first_gids.len()).rev().find(|tileset_index| self.tileset_first_gids[*tileset_index] <= gid)
	}

	//==============================================================================
	/// 층 넣기. (맨 위에 그린다)
	///
	/// # 인자
	/// - `layer`: 넣을 층.
	///
	/// # 돌려줌
	/// 층 번호.
	//==============================================================================
	pub fn add_layer(&mut self, layer: TilemapLayer) -> usize {
		self.layers.push(layer);
		self.layers.len() - 1
	}

	//==============================================================================
	/// 층 끼우기. (그 번호 앞에, 앞의 층일수록 먼저(아래에) 그린다)
	///
	/// # 인자
	/// - `layer_index`: 끼울 번호. (층 수보다 크면 맨 위)
	/// - `layer`: 끼울 층.
	///
	/// # 돌려줌
	/// 끼운 층 번호.
	//==============================================================================
	pub fn insert_layer(&mut self, layer_index: usize, layer: TilemapLayer) -> usize {
		let insert_index = layer_index.min(self.layers.len());
		self.layers.insert(insert_index, layer);
		insert_index
	}

	//==============================================================================
	/// 새 층 만들어 넣기. (덩이 한 변 16 칸)
	///
	/// # 인자
	/// - `name`: 이름.
	///
	/// # 돌려줌
	/// 새 층.
	//==============================================================================
	pub fn create_layer(&mut self, name: &str) -> &mut TilemapLayer {
		let layer_index = self.add_layer(TilemapLayer::new(name));
		&mut self.layers[layer_index]
	}

	//==============================================================================
	/// 층 빼기.
	///
	/// # 인자
	/// - `layer_index`: 층 번호. (없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn remove_layer(&mut self, layer_index: usize) {
		if layer_index < self.layers.len() {
			self.layers.remove(layer_index);
		}
	}

	//==============================================================================
	/// 층 목록 반환.
	///
	/// # 돌려줌
	/// 층. (그리는 차례)
	//==============================================================================
	pub fn get_layers(&self) -> &[TilemapLayer] {
		&self.layers
	}

	//==============================================================================
	/// 층 수 반환.
	///
	/// # 돌려줌
	/// 층 수.
	//==============================================================================
	pub fn get_layer_count(&self) -> usize {
		self.layers.len()
	}

	//==============================================================================
	/// 층 반환.
	///
	/// # 인자
	/// - `layer_index`: 층 번호.
	///
	/// # 돌려줌
	/// 층. (없으면 None)
	//==============================================================================
	pub fn get_layer(&self, layer_index: usize) -> Option<&TilemapLayer> {
		self.layers.get(layer_index)
	}

	//==============================================================================
	/// 층 반환. (변경용)
	///
	/// # 인자
	/// - `layer_index`: 층 번호.
	///
	/// # 돌려줌
	/// 층. (없으면 None)
	//==============================================================================
	pub fn get_layer_mut(&mut self, layer_index: usize) -> Option<&mut TilemapLayer> {
		self.layers.get_mut(layer_index)
	}

	//==============================================================================
	/// 이름으로 층 번호 찾기.
	///
	/// # 인자
	/// - `name`: 층 이름.
	///
	/// # 돌려줌
	/// 처음 맞는 층 번호. (없으면 None)
	//==============================================================================
	pub fn find_layer_index(&self, name: &str) -> Option<usize> {
		self.layers.iter().position(|layer| layer.get_name() == name)
	}

	//==============================================================================
	/// 애니메이션 시계 설정.
	///
	/// # 인자
	/// - `animation_time`: 시각. (초)
	//==============================================================================
	pub fn set_animation_time(&mut self, animation_time: f64) {
		self.animation_time = animation_time;
	}

	//==============================================================================
	/// 애니메이션 시계 반환.
	///
	/// # 돌려줌
	/// 시각. (초, 처음은 0)
	//==============================================================================
	pub fn get_animation_time(&self) -> f64 {
		self.animation_time
	}

	//==============================================================================
	/// 애니메이션 빠르기 배율 설정. (유니티 Tilemap.animationFrameRate 같은 배율)
	///
	/// # 인자
	/// - `animation_speed`: 배율. (0 이면 멈춘다)
	//==============================================================================
	pub fn set_animation_speed(&mut self, animation_speed: f32) {
		self.animation_speed = animation_speed;
	}

	//==============================================================================
	/// 애니메이션 빠르기 배율 반환.
	///
	/// # 돌려줌
	/// 배율. (처음은 1)
	//==============================================================================
	pub fn get_animation_speed(&self) -> f32 {
		self.animation_speed
	}

	//==============================================================================
	/// 지난 그리기에서 그린 덩이 수 반환.
	///
	/// # 돌려줌
	/// 덩이 수. (모든 층의 합)
	//==============================================================================
	pub fn get_last_drawn_chunk_count(&self) -> u32 {
		self.last_drawn_chunk_count
	}

	//==============================================================================
	/// 지난 그리기에서 화면 밖이라 건너뛴 덩이 수 반환.
	///
	/// # 돌려줌
	/// 덩이 수. (모든 층의 합)
	//==============================================================================
	pub fn get_last_culled_chunk_count(&self) -> u32 {
		self.last_culled_chunk_count
	}

	//==============================================================================
	/// 칸을 감싸는 사각형의 왼쪽 위. (지도 좌표, Tiled tileToScreenCoords 의 칸 자리)
	///
	/// # 인자
	/// - `column`: 열.
	/// - `row`: 행.
	///
	/// # 돌려줌
	/// 왼쪽 위. (픽셀, 노드 내용 좌표)
	//==============================================================================
	pub fn cell_to_map_position(&self, column: i32, row: i32) -> Vector2 {
		let layout = self.create_layout();
		Vector2::create(layout.read_cell_left(column, row), layout.read_cell_top(column, row))
	}

	//==============================================================================
	/// 칸의 가운데. (지도 좌표)
	///
	/// # 인자
	/// - `column`: 열.
	/// - `row`: 행.
	///
	/// # 돌려줌
	/// 가운데. (픽셀, 노드 내용 좌표)
	//==============================================================================
	pub fn cell_to_map_center(&self, column: i32, row: i32) -> Vector2 {
		let layout = self.create_layout();
		let left = layout.read_cell_left(column, row);
		let top = layout.read_cell_top(column, row);
		Vector2::create(left + layout.read_cell_width() / 2.0, top + layout.read_cell_height() / 2.0)
	}

	//==============================================================================
	/// 지도 좌표가 든 칸 찾기. (Tiled screenToTileCoords)
	/// - 정사각과 아이소는 격자 나눗셈, 육각은 가까운 칸 가운데, 엇갈림은 마름모 모서리 판정이다.
	///
	/// # 인자
	/// - `x`: 지도 X. (픽셀, 노드 내용 좌표)
	/// - `y`: 지도 Y. (픽셀, 노드 내용 좌표)
	///
	/// # 돌려줌
	/// 칸.
	//==============================================================================
	pub fn map_position_to_cell(&self, x: f32, y: f32) -> GridCell {
		let layout = self.create_layout();
		match layout.orientation {
			TilemapOrientation::Isometric => {
				let shifted_x = x as f64 - layout.origin_x as f64;
				let tile_y = y as f64 / layout.tile_height as f64;
				let tile_x = shifted_x / layout.tile_width as f64;
				GridCell::create((tile_y + tile_x).floor() as i32, (tile_y - tile_x).floor() as i32)
			}
			TilemapOrientation::Hexagonal => {
				Tilemap::find_hexagonal_cell(&layout, x as f64, y as f64)
			}
			TilemapOrientation::Staggered => {
				Tilemap::find_staggered_cell(&layout, x as f64, y as f64)
			}
			TilemapOrientation::Orthogonal => {
				GridCell::create((x as f64 / layout.tile_width as f64).floor() as i32, (y as f64 / layout.tile_height as f64).floor() as i32)
			}
		}
	}

	//==============================================================================
	/// 육각 배치에서 지도 좌표가 든 칸 찾기. (Tiled HexagonalRenderer::screenToTileCoords)
	//==============================================================================
	fn find_hexagonal_cell(layout: &TilemapLayout, x: f64, y: f64) -> GridCell {
		let mut shifted_x = x;
		let mut shifted_y = y;
		if layout.is_stagger_x {
			shifted_x -= if layout.is_stagger_even {
				layout.hexagon_tile_width as f64
			}
			else {
				layout.side_offset_x as f64
			};
		}
		else {
			shifted_y -= if layout.is_stagger_even {
				layout.hexagon_tile_height as f64
			}
			else {
				layout.side_offset_y as f64
			};
		}
		let column_width = layout.column_width as f64;
		let row_height = layout.row_height as f64;
		let mut reference_column = (shifted_x / (column_width * 2.0)).floor() as i32;
		let mut reference_row = (shifted_y / (row_height * 2.0)).floor() as i32;
		let relative_x = shifted_x - reference_column as f64 * (column_width * 2.0);
		let relative_y = shifted_y - reference_row as f64 * (row_height * 2.0);
		let stagger_addition = if layout.is_stagger_even {
			1
		}
		else {
			0
		};
		let center_list: [(f64, f64); 4];
		let offset_list: [(i32, i32); 4];
		if layout.is_stagger_x {
			reference_column = reference_column * 2 + stagger_addition;
			let left = (layout.side_length_x as i32 / 2) as f64;
			let center_x = left + column_width;
			let center_y = (layout.hexagon_tile_height as i32 / 2) as f64;
			center_list = [(left, center_y), (center_x, center_y - row_height), (center_x, center_y + row_height), (center_x + column_width, center_y)];
			offset_list = [(0, 0), (1, -1), (1, 0), (2, 0)];
		}
		else {
			reference_row = reference_row * 2 + stagger_addition;
			let top = (layout.side_length_y as i32 / 2) as f64;
			let center_x = (layout.hexagon_tile_width as i32 / 2) as f64;
			let center_y = top + row_height;
			center_list = [(center_x, top), (center_x - column_width, center_y), (center_x + column_width, center_y), (center_x, center_y + row_height)];
			offset_list = [(0, 0), (-1, 1), (0, 1), (0, 2)];
		}
		let mut nearest_index = 0;
		let mut nearest_distance = f64::INFINITY;
		for (center_index, (center_x, center_y)) in center_list.iter().enumerate() {
			let difference_x = center_x - relative_x;
			let difference_y = center_y - relative_y;
			let distance = difference_x * difference_x + difference_y * difference_y;
			if distance < nearest_distance {
				nearest_distance = distance;
				nearest_index = center_index;
			}
		}
		GridCell::create(reference_column + offset_list[nearest_index].0, reference_row + offset_list[nearest_index].1)
	}

	//==============================================================================
	/// 엇갈림 배치에서 지도 좌표가 든 칸 찾기. (Tiled StaggeredRenderer::screenToTileCoords)
	//==============================================================================
	fn find_staggered_cell(layout: &TilemapLayout, x: f64, y: f64) -> GridCell {
		let mut aligned_x = x;
		let mut aligned_y = y;
		if layout.is_stagger_x {
			if layout.is_stagger_even {
				aligned_x -= layout.side_offset_x as f64;
			}
		}
		else if layout.is_stagger_even {
			aligned_y -= layout.side_offset_y as f64;
		}
		let tile_width = layout.hexagon_tile_width as f64;
		let tile_height = layout.hexagon_tile_height as f64;
		let mut column = (aligned_x / tile_width).floor() as i32;
		let mut row = (aligned_y / tile_height).floor() as i32;
		let relative_x = aligned_x - column as f64 * tile_width;
		let relative_y = aligned_y - row as f64 * tile_height;
		let stagger_addition = if layout.is_stagger_even {
			1
		}
		else {
			0
		};
		if layout.is_stagger_x {
			column = column * 2 + stagger_addition;
		}
		else {
			row = row * 2 + stagger_addition;
		}
		let side_offset_y = layout.side_offset_y as f64;
		let slope_y = relative_x * (tile_height / tile_width);
		let mut cell = GridCell::create(column, row);
		if side_offset_y - slope_y > relative_y {
			cell = Tilemap::find_staggered_neighbor(layout, cell, -1, -1);
		}
		if -side_offset_y + slope_y > relative_y {
			cell = Tilemap::find_staggered_neighbor(layout, cell, 1, -1);
		}
		if side_offset_y + slope_y < relative_y {
			cell = Tilemap::find_staggered_neighbor(layout, cell, -1, 1);
		}
		if side_offset_y * 3.0 - slope_y < relative_y {
			cell = Tilemap::find_staggered_neighbor(layout, cell, 1, 1);
		}
		cell
	}

	//==============================================================================
	/// 엇갈린 배치의 대각 이웃 칸. (Tiled HexagonalRenderer 의 topLeft, topRight, bottomLeft, bottomRight)
	//==============================================================================
	fn find_staggered_neighbor(layout: &TilemapLayout, cell: GridCell, horizontal_sign: i32, vertical_sign: i32) -> GridCell {
		if !layout.is_stagger_x {
			let is_staggered = layout.is_staggered_index(cell.row);
			let mut column = cell.column;
			if horizontal_sign < 0 && !is_staggered {
				column -= 1;
			}
			else if horizontal_sign > 0 && is_staggered {
				column += 1;
			}
			return GridCell::create(column, cell.row + vertical_sign);
		}
		let is_staggered = layout.is_staggered_index(cell.column);
		let mut row = cell.row;
		if vertical_sign < 0 && !is_staggered {
			row -= 1;
		}
		else if vertical_sign > 0 && is_staggered {
			row += 1;
		}
		GridCell::create(cell.column + horizontal_sign, row)
	}

	//==============================================================================
	/// 지도 크기 셈하기. (set_map_size 의 칸 수로, Tiled mapBoundingRect 의 크기)
	///
	/// # 돌려줌
	/// 너비와 높이. (픽셀)
	//==============================================================================
	pub fn read_map_pixel_size(&self) -> Vector2 {
		let layout = self.create_layout();
		let bounds = layout.read_cell_range_bounds(0, 0, self.column_count as i32, self.row_count as i32);
		bounds.size
	}

	//==============================================================================
	/// 칸 하나를 그리는 자리 셈하기. (Tiled CellRenderer::render, 그림 영역이 비면 None)
	//==============================================================================
	fn read_tile_placement(layout: &TilemapLayout, tileset: &Tileset, tile_id: u32, tile_flags: u32, column: i32, row: i32) -> Option<TilePlacement> {
		let mut source_x = 0.0;
		let mut source_y = 0.0;
		let mut source_width = 0.0;
		let mut source_height = 0.0;
		if let Some(image_rect) = tileset.get_tile_image_rect(tile_id) {
			source_x = image_rect.position.x;
			source_y = image_rect.position.y;
			source_width = image_rect.size.x;
			source_height = image_rect.size.y;
		}
		else if !tileset.has_own_tile_image(tile_id) && tileset.get_columns() > 0 {
			let columns = tileset.get_columns();
			let tile_width = tileset.get_tile_width();
			let tile_height = tileset.get_tile_height();
			let margin = tileset.get_margin();
			let spacing = tileset.get_spacing();
			source_x = (margin + (tile_id % columns) * (tile_width + spacing)) as f32;
			source_y = (margin + (tile_id / columns) * (tile_height + spacing)) as f32;
			source_width = tile_width as f32;
			source_height = tile_height as f32;
		}
		else if let Some(tile_image) = tileset.get_tile_image(tile_id) {
			source_width = tile_image.get_width() as f32;
			source_height = tile_image.get_height() as f32;
		}
		if source_width <= 0.0 || source_height <= 0.0 {
			return None;
		}

		// 그리는 크기와 배율.
		let mut render_width = source_width;
		let mut render_height = source_height;
		if tileset.get_tile_render_size() == TileRenderSize::Grid {
			render_width = layout.tile_width;
			render_height = layout.tile_height;
		}
		let mut scale_x = render_width / source_width;
		let mut scale_y = render_height / source_height;
		if tileset.get_fill_mode() == TileFillMode::PreserveAspectFit {
			let minimum_scale = scale_x.min(scale_y);
			scale_x = minimum_scale;
			scale_y = minimum_scale;
		}

		// 칸의 왼쪽 아래에 맞춘 가운데. (오프셋도 배율을 곱한다)
		let cell_left = layout.read_cell_left(column, row);
		let cell_bottom = layout.read_cell_top(column, row) + layout.read_cell_height();
		let tile_offset = tileset.get_tile_offset();
		let mut center_x = cell_left + render_width / 2.0 + tile_offset.x * scale_x;
		let mut center_y = cell_bottom - render_height / 2.0 + tile_offset.y * scale_y;

		// 뒤집기와 돌리기.
		let is_flag_horizontal = (tile_flags & TileFlag::HORIZONTAL_FLIP) != 0;
		let is_flag_vertical = (tile_flags & TileFlag::VERTICAL_FLIP) != 0;
		let is_flag_diagonal = (tile_flags & TileFlag::DIAGONAL_FLIP) != 0;
		let mut is_horizontal_flip = is_flag_horizontal;
		let mut is_vertical_flip = is_flag_vertical;
		let mut rotation = 0.0;
		if layout.orientation == TilemapOrientation::Hexagonal {
			if is_flag_diagonal {
				rotation += 60.0;
			}
			if (tile_flags & TileFlag::HEXAGONAL_ROTATION_120) != 0 {
				rotation += 120.0;
			}
		}
		else if is_flag_diagonal {
			rotation = 90.0;
			is_horizontal_flip = is_flag_vertical;
			is_vertical_flip = !is_flag_horizontal;
			let half_difference = render_height / 2.0 - render_width / 2.0;
			center_x += half_difference;
			center_y += half_difference;
		}
		Some(TilePlacement {
			source_x,
			source_y,
			source_width,
			source_height,
			scale_x,
			scale_y,
			center_x,
			center_y,
			is_horizontal_flip,
			is_vertical_flip,
			rotation,
		})
	}

	//==============================================================================
	/// 칸 하나의 충돌 모양 셈하기. (지도 좌표, 층 오프셋과 뒤집기, 돌리기, 타일 오프셋을 건다)
	/// - 애니메이션 타일은 지도에 놓인 번호(첫 프레임이 아닌 놓인 타일)의 모양을 쓴다.
	///
	/// # 인자
	/// - `layer_index`: 층 번호.
	/// - `column`: 열.
	/// - `row`: 행.
	///
	/// # 돌려줌
	/// 모양 목록. (빈 칸이거나 모양이 없으면 빈 목록)
	//==============================================================================
	pub fn read_cell_collision_shapes(&self, layer_index: usize, column: i32, row: i32) -> Vec<TileCollisionShape> {
		let mut shape_list = Vec::new();
		if let Some(layer) = self.layers.get(layer_index) {
			let layout = self.create_layout();
			let tile = layer.get_tile(column, row);
			self.append_cell_collision_shapes(&layout, layer, tile, column, row, &mut shape_list);
		}
		shape_list
	}

	//==============================================================================
	/// 층 전체의 충돌 모양 셈하기. (정적 충돌 지형을 한 번에 지을 때)
	///
	/// # 인자
	/// - `layer_index`: 층 번호.
	///
	/// # 돌려줌
	/// 모양 목록. (덩이의 행, 열 차례, 덩이 안은 행 차례)
	//==============================================================================
	pub fn read_layer_collision_shapes(&self, layer_index: usize) -> Vec<TileCollisionShape> {
		let mut shape_list = Vec::new();
		if let Some(layer) = self.layers.get(layer_index) {
			let layout = self.create_layout();
			let chunk_size = layer.get_chunk_size();
			for chunk in layer.get_chunks() {
				for (tile_index, tile) in chunk.tiles.iter().enumerate() {
					if *tile == 0 {
						continue;
					}
					let column = chunk.column * chunk_size + tile_index as i32 % chunk_size;
					let row = chunk.row * chunk_size + tile_index as i32 / chunk_size;
					self.append_cell_collision_shapes(&layout, layer, *tile, column, row, &mut shape_list);
				}
			}
		}
		shape_list
	}

	//==============================================================================
	/// 칸 하나의 충돌 모양을 목록에 더하기.
	//==============================================================================
	fn append_cell_collision_shapes(&self, layout: &TilemapLayout, layer: &TilemapLayer, tile: u32, column: i32, row: i32, shape_list: &mut Vec<TileCollisionShape>) {
		let tileset_index = match self.find_tileset_index(tile) {
			Some(tileset_index) => {
				tileset_index
			}
			None => {
				return;
			}
		};
		let tileset = &self.tilesets[tileset_index];
		let tile_id = Tilemap::read_tile_id(tile) - self.tileset_first_gids[tileset_index];
		let collision_shapes = tileset.get_tile_collision_shapes(tile_id);
		if collision_shapes.is_empty() {
			return;
		}
		let placement = match Tilemap::read_tile_placement(layout, tileset, tile_id, Tilemap::read_tile_flags(tile), column, row) {
			Some(placement) => {
				placement
			}
			None => {
				return;
			}
		};
		let layer_offset = layer.get_offset();
		let horizontal_sign = if placement.is_horizontal_flip {
			-1.0
		}
		else {
			1.0
		};
		let vertical_sign = if placement.is_vertical_flip {
			-1.0
		}
		else {
			1.0
		};
		let mut transform_matrix = TransformMatrix::identity();
		transform_matrix.translate(layer_offset.x + placement.center_x, layer_offset.y + placement.center_y);
		transform_matrix.rotate(placement.rotation.to_radians());
		transform_matrix.scale(placement.scale_x * horizontal_sign, placement.scale_y * vertical_sign);
		transform_matrix.translate(-placement.source_width / 2.0, -placement.source_height / 2.0);
		for collision_shape in collision_shapes.iter() {
			shape_list.push(collision_shape.create_transformed(&transform_matrix));
		}
	}

	//==============================================================================
	/// 그리기 여유 셈하기. (격자보다 큰 타일, 오프셋)
	//==============================================================================
	fn read_draw_margin(&self, layout: &TilemapLayout) -> f32 {
		let mut draw_margin = layout.tile_width.max(layout.tile_height);
		for tileset in self.tilesets.iter() {
			draw_margin = draw_margin.max(tileset.read_maximum_tile_extent());
		}
		draw_margin
	}

	//==============================================================================
	/// 화면에 걸치는 덩이 고르기. (덩이를 감싸는 사각형을 여유만큼 넓혀 본다)
	//==============================================================================
	fn select_visible_chunks<'layer>(layout: &TilemapLayout, layer: &'layer TilemapLayer, draw_margin: f32, is_rect_visible: &mut dyn FnMut(Rect) -> bool) -> Vec<&'layer TilemapChunk> {
		let chunk_size = layer.get_chunk_size();
		let mut visible_chunk_list = Vec::new();
		for chunk in layer.get_chunks() {
			let chunk_bounds = layout.read_cell_range_bounds(chunk.column * chunk_size, chunk.row * chunk_size, chunk_size, chunk_size);
			let inflated_bounds = Rect::create(chunk_bounds.position.x - draw_margin, chunk_bounds.position.y - draw_margin, chunk_bounds.size.x + draw_margin * 2.0, chunk_bounds.size.y + draw_margin * 2.0);
			if is_rect_visible(inflated_bounds) {
				visible_chunk_list.push(chunk);
			}
		}
		visible_chunk_list
	}

	//==============================================================================
	/// 그릴 칸을 그리는 차례로 모으기. (Chunk 모드는 덩이 차례 안에서, Individual 모드는 지도 전체 차례로)
	//==============================================================================
	fn collect_draw_cells(&self, layout: &TilemapLayout, chunk_size: i32, visible_chunk_list: &mut [&TilemapChunk], draw_cell_list: &mut Vec<(i32, i32, u32)>) {
		if visible_chunk_list.is_empty() {
			return;
		}
		if self.render_mode == TilemapRenderMode::Individual {
			let minimum_chunk_column = visible_chunk_list.iter().map(|chunk| chunk.column).min().unwrap_or(0);
			let minimum_chunk_row = visible_chunk_list.iter().map(|chunk| chunk.row).min().unwrap_or(0);
			let maximum_chunk_column = visible_chunk_list.iter().map(|chunk| chunk.column).max().unwrap_or(0);
			let maximum_chunk_row = visible_chunk_list.iter().map(|chunk| chunk.row).max().unwrap_or(0);
			let chunk_column_span = maximum_chunk_column - minimum_chunk_column + 1;
			let chunk_row_span = maximum_chunk_row - minimum_chunk_row + 1;
			let mut chunk_grid: Vec<Option<&TilemapChunk>> = vec![None; (chunk_column_span * chunk_row_span) as usize];
			for chunk in visible_chunk_list.iter() {
				chunk_grid[((chunk.row - minimum_chunk_row) * chunk_column_span + (chunk.column - minimum_chunk_column)) as usize] = Some(*chunk);
			}
			let first_column = minimum_chunk_column * chunk_size;
			let first_row = minimum_chunk_row * chunk_size;
			layout.visit_cells_in_draw_order(first_column, first_row, chunk_column_span * chunk_size, chunk_row_span * chunk_size, &mut |column: i32, row: i32| {
				let grid_column = (column - first_column).div_euclid(chunk_size);
				let grid_row = (row - first_row).div_euclid(chunk_size);
				if let Some(chunk) = chunk_grid[(grid_row * chunk_column_span + grid_column) as usize] {
					let tile = chunk.tiles[((row - chunk.row * chunk_size) * chunk_size + (column - chunk.column * chunk_size)) as usize];
					if tile != 0 {
						draw_cell_list.push((column, row, tile));
					}
				}
			});
			return;
		}
		layout.sort_chunks_in_draw_order(visible_chunk_list);
		for chunk in visible_chunk_list.iter() {
			let chunk_left = chunk.column * chunk_size;
			let chunk_top = chunk.row * chunk_size;
			layout.visit_cells_in_draw_order(chunk_left, chunk_top, chunk_size, chunk_size, &mut |column: i32, row: i32| {
				let tile = chunk.tiles[((row - chunk_top) * chunk_size + (column - chunk_left)) as usize];
				if tile != 0 {
					draw_cell_list.push((column, row, tile));
				}
			});
		}
	}

	//==============================================================================
	/// 층을 모두 그리기. (그린 덩이 수, 건너뛴 덩이 수를 돌려준다)
	//==============================================================================
	fn draw_layers(&self, graphic: &mut Graphic) -> (u32, u32) {
		let mut drawn_chunk_count = 0;
		let mut culled_chunk_count = 0;
		if self.layers.is_empty() || self.tilesets.is_empty() {
			return (0, 0);
		}
		let layout = self.create_layout();
		let draw_margin = self.read_draw_margin(&layout);
		let vertex_float_count = graphic.get_vertex_data().len();
		let mut draw_state = TilemapDrawState {
			float_capacity: vertex_float_count - vertex_float_count % FLOATS_PER_QUAD,
			float_offset: 0,
			image: None,
			texture: None,
			color: Color::white(),
			time_milliseconds: self.animation_time * 1000.0,
			tileset_index: 0,
			tileset_first_gid: 1,
			tileset_last_gid: 0,
		};
		let mut draw_cell_list: Vec<(i32, i32, u32)> = Vec::new();
		for layer in self.layers.iter() {
			if !layer.is_visible() || layer.get_opacity() <= 0.0 || layer.get_chunk_count() == 0 {
				continue;
			}
			let layer_color = layer.get_color();
			let layer_offset = layer.get_offset();
			let chunk_size = layer.get_chunk_size();
			draw_state.color = Color::create(layer_color.red, layer_color.green, layer_color.blue, layer_color.alpha * layer.get_opacity());
			draw_state.image = None;
			draw_state.texture = None;
			draw_state.float_offset = 0;
			graphic.push_state();
			graphic.translate(layer_offset.x, layer_offset.y);
			graphic.set_blend_mode(layer.get_blend_mode());

			// 화면에 걸치는 덩이 고르기.
			let mut visible_chunk_list = Tilemap::select_visible_chunks(&layout, layer, draw_margin, &mut |rect: Rect| graphic.is_local_rect_visible(rect));
			drawn_chunk_count += visible_chunk_list.len() as u32;
			culled_chunk_count += (layer.get_chunk_count() - visible_chunk_list.len()) as u32;
			draw_cell_list.clear();
			self.collect_draw_cells(&layout, chunk_size, &mut visible_chunk_list, &mut draw_cell_list);
			for (column, row, tile) in draw_cell_list.iter() {
				self.draw_cell(graphic, &layout, &mut draw_state, *tile, *column, *row);
			}
			Tilemap::flush_vertices(graphic, &mut draw_state);
			graphic.set_blend_mode(BlendMode::Normal);
			graphic.pop_state();
		}
		(drawn_chunk_count, culled_chunk_count)
	}

	//==============================================================================
	/// 칸 하나를 꼭짓점 버퍼에 적기. (그림이 바뀌거나 버퍼가 차면 먼저 그린다)
	//==============================================================================
	fn draw_cell(&self, graphic: &mut Graphic, layout: &TilemapLayout, draw_state: &mut TilemapDrawState, tile: u32, column: i32, row: i32) {
		let gid = tile & TILE_ID_MASK;

		// 타일셋 찾기. (이웃 칸은 거의 같은 타일셋이라 지난 범위부터 본다)
		if gid < draw_state.tileset_first_gid || gid > draw_state.tileset_last_gid {
			let tileset_index = match self.find_tileset_index(gid) {
				Some(tileset_index) => {
					tileset_index
				}
				None => {
					return;
				}
			};
			let next_first_gid = self.get_tileset_first_gid(tileset_index + 1);
			draw_state.tileset_index = tileset_index;
			draw_state.tileset_first_gid = self.tileset_first_gids[tileset_index];
			draw_state.tileset_last_gid = if next_first_gid > 0 {
				next_first_gid - 1
			}
			else {
				u32::MAX
			};
		}
		let tileset = &self.tilesets[draw_state.tileset_index];
		let base_tile_id = gid - draw_state.tileset_first_gid;
		let tile_id = tileset.read_animated_tile_id(base_tile_id, draw_state.time_milliseconds);
		let image = match tileset.get_tile_image(tile_id) {
			Some(image) => {
				image.clone()
			}
			None => {
				return;
			}
		};
		if image.get_width() == 0 || image.get_height() == 0 {
			return;
		}
		let placement = match Tilemap::read_tile_placement(layout, tileset, tile_id, tile & TILE_FLAG_BITS, column, row) {
			Some(placement) => {
				placement
			}
			None => {
				return;
			}
		};

		// 그림이 바뀌면 쌓인 것을 그리고 텍스처를 바꾼다.
		let is_same_image = match &draw_state.image {
			Some(current_image) => {
				Rc::ptr_eq(current_image, &image)
			}
			None => {
				false
			}
		};
		if !is_same_image {
			Tilemap::flush_vertices(graphic, draw_state);
			draw_state.texture = graphic.get_image_texture(&image);
			draw_state.image = Some(image.clone());
		}
		if draw_state.texture.is_none() {
			return;
		}
		if draw_state.float_offset + FLOATS_PER_QUAD > draw_state.float_capacity {
			Tilemap::flush_vertices(graphic, draw_state);
		}
		let quad_values = placement.write_quad(image.get_width() as f32, image.get_height() as f32);
		let float_offset = draw_state.float_offset;
		graphic.get_vertex_data()[float_offset..float_offset + FLOATS_PER_QUAD].copy_from_slice(&quad_values);
		draw_state.float_offset = float_offset + FLOATS_PER_QUAD;
	}

	//==============================================================================
	/// 쌓인 꼭짓점 그리기. (쌓인 것이 없으면 아무것도 하지 않는다)
	//==============================================================================
	fn flush_vertices(graphic: &mut Graphic, draw_state: &mut TilemapDrawState) {
		if draw_state.float_offset == 0 {
			return;
		}
		if let Some(texture) = draw_state.texture {
			graphic.draw_vertices(draw_state.float_offset / FLOATS_PER_VERTEX, texture, draw_state.color);
		}
		draw_state.float_offset = 0;
	}
}

impl Default for Tilemap {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Tilemap {
		Tilemap::new()
	}
}

impl Component for Tilemap {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Tilemap"
	}

	/// 애니메이션 시계를 흘린다.
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f32) {
		self.animation_time += time_delta as f64 * self.animation_speed as f64;
	}

	/// 층을 넣은 차례로, 층마다 화면에 걸치는 덩이만 그린다.
	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		let (drawn_chunk_count, culled_chunk_count) = self.draw_layers(graphic);
		self.last_drawn_chunk_count = drawn_chunk_count;
		self.last_culled_chunk_count = culled_chunk_count;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental::tilemap::tileset::{TileAnimationFrame, TileCollisionShapeKind};

	//==============================================================================
	/// 소수 넷째 자리까지 같은지.
	//==============================================================================
	fn assert_near(actual_value: f32, expected_value: f32) {
		assert!((actual_value - expected_value).abs() < 0.0001, "{} != {}", actual_value, expected_value);
	}

	//==============================================================================
	/// 모양을 견줄 수 있는 값으로. (종류, x, y, 너비, 높이, 회전, 꼭짓점)
	//==============================================================================
	fn assert_shape(shape: &TileCollisionShape, kind: TileCollisionShapeKind, values: [f32; 6], points: &[(f32, f32)]) {
		assert_eq!(shape.kind, kind);
		assert_near(shape.x, values[0]);
		assert_near(shape.y, values[1]);
		assert_near(shape.width, values[2]);
		assert_near(shape.height, values[3]);
		assert_near(shape.rotation, values[4]);
		assert_eq!(values[5] as usize, shape.points.len());
		for (point, (expected_x, expected_y)) in shape.points.iter().zip(points.iter()) {
			assert_near(point.x, *expected_x);
			assert_near(point.y, *expected_y);
		}
	}

	//==============================================================================
	/// 타일셋 격자. (원본 check.mjs 의 tilesetGrid)
	//==============================================================================
	#[test]
	fn tileset_grid() {
		let mut tileset = Tileset::new();
		tileset.set_tile_size(16, 16);
		tileset.set_margin(1);
		tileset.set_spacing(2);
		tileset.fit_grid_to_image_size(144, 54);
		let image_rect = tileset.read_tile_image_rect(21);
		assert_eq!((tileset.get_columns(), tileset.get_tile_count()), (8, 24));
		assert_eq!(image_rect, Rect::create(91.0, 37.0, 16.0, 16.0));
	}

	//==============================================================================
	/// 애니메이션 시각. (원본 animation, animationNone)
	//==============================================================================
	#[test]
	fn animation_time() {
		let mut tileset = Tileset::new();
		tileset.set_tile_animation(3, &[TileAnimationFrame::new(3, 180), TileAnimationFrame::new(4, 180), TileAnimationFrame::new(5, 180), TileAnimationFrame::new(6, 180)]);
		let frame_list: Vec<u32> = [0.0, 179.0, 180.0, 719.0, 720.0, -1.0, 1000000.0].iter().map(|time| tileset.read_animated_tile_id(3, *time)).collect();
		assert_eq!(frame_list, vec![3, 3, 4, 6, 3, 6, 6]);
		assert_eq!(tileset.read_animated_tile_id(9, 500.0), 9);
	}

	//==============================================================================
	/// 뒤집기와 돌리기 비트. (원본 rotateTile, flipAfterRotate)
	//==============================================================================
	#[test]
	fn tile_flags() {
		let rotated_list: Vec<u32> = [0, 1, 2, 3, 4, -1].iter().map(|turn_count| Tilemap::rotate_tile(5, *turn_count)).collect();
		assert_eq!(rotated_list, vec![5, 5 | TileFlag::DIAGONAL_FLIP | TileFlag::HORIZONTAL_FLIP, 5 | TileFlag::HORIZONTAL_FLIP | TileFlag::VERTICAL_FLIP, 5 | TileFlag::DIAGONAL_FLIP | TileFlag::VERTICAL_FLIP, 5, 5 | TileFlag::DIAGONAL_FLIP | TileFlag::VERTICAL_FLIP]);
		assert_eq!(Tilemap::flip_tile_horizontally(Tilemap::rotate_tile(5, 1)), 5 | TileFlag::DIAGONAL_FLIP);
		assert_eq!(Tilemap::flip_tile_vertically(5), 5 | TileFlag::VERTICAL_FLIP);
		assert_eq!(Tilemap::read_tile_id(0xE000_0007), 7);
		assert_eq!(Tilemap::read_tile_flags(0x9000_0007), 0x9000_0000);
	}

	//==============================================================================
	/// 좌표 바꾸기: 정사각, 아이소. (원본 orthogonal, isometric)
	//==============================================================================
	#[test]
	fn orthogonal_and_isometric_coordinates() {
		let mut orthogonal_map = Tilemap::new();
		orthogonal_map.set_tile_size(16, 16);
		orthogonal_map.set_map_size(18, 12);
		assert_eq!(orthogonal_map.cell_to_map_position(3, 2), Vector2::create(48.0, 32.0));
		assert_eq!(orthogonal_map.map_position_to_cell(-1.0, -1.0), GridCell::create(-1, -1));
		assert_eq!(orthogonal_map.read_map_pixel_size(), Vector2::create(288.0, 192.0));

		let mut isometric_map = Tilemap::new();
		isometric_map.set_orientation(TilemapOrientation::Isometric);
		isometric_map.set_tile_size(64, 32);
		isometric_map.set_map_size(10, 10);
		let center = isometric_map.cell_to_map_center(2, 3);
		assert_eq!(isometric_map.cell_to_map_position(2, 3), Vector2::create(256.0, 80.0));
		assert_eq!(center, Vector2::create(288.0, 96.0));
		assert_eq!(isometric_map.map_position_to_cell(center.x, center.y), GridCell::create(2, 3));
		assert_eq!(isometric_map.map_position_to_cell(320.0, 1.0), GridCell::create(0, 0));
		assert_eq!(isometric_map.read_map_pixel_size(), Vector2::create(640.0, 320.0));
	}

	//==============================================================================
	/// 좌표 바꾸기: 육각 Y 축 홀수, X 축 짝수. (원본 hexagonalY, hexagonalX)
	//==============================================================================
	#[test]
	fn hexagonal_coordinates() {
		let mut hexagonal_map = Tilemap::new();
		hexagonal_map.set_orientation(TilemapOrientation::Hexagonal);
		hexagonal_map.set_tile_size(32, 32);
		hexagonal_map.set_hex_side_length(16);
		hexagonal_map.set_map_size(8, 6);
		let center = hexagonal_map.cell_to_map_center(1, 1);
		assert_eq!(hexagonal_map.cell_to_map_position(1, 1), Vector2::create(48.0, 24.0));
		assert_eq!(center, Vector2::create(64.0, 40.0));
		assert_eq!(hexagonal_map.map_position_to_cell(center.x, center.y), GridCell::create(1, 1));
		assert_eq!(hexagonal_map.map_position_to_cell(1.0, 1.0), GridCell::create(-1, -1));
		assert_eq!(hexagonal_map.read_map_pixel_size(), Vector2::create(272.0, 152.0));

		hexagonal_map.set_stagger_axis(TilemapStaggerAxis::X);
		hexagonal_map.set_stagger_index(TilemapStaggerIndex::Even);
		let x_center = hexagonal_map.cell_to_map_center(3, 2);
		assert_eq!(hexagonal_map.cell_to_map_position(2, 1), Vector2::create(48.0, 48.0));
		assert_eq!(x_center, Vector2::create(88.0, 80.0));
		assert_eq!(hexagonal_map.map_position_to_cell(x_center.x, x_center.y), GridCell::create(3, 2));
		assert_eq!(hexagonal_map.read_map_pixel_size(), Vector2::create(200.0, 208.0));
	}

	//==============================================================================
	/// 좌표 바꾸기: 엇갈림 아이소. (원본 staggered)
	//==============================================================================
	#[test]
	fn staggered_coordinates() {
		let mut staggered_map = Tilemap::new();
		staggered_map.set_orientation(TilemapOrientation::Staggered);
		staggered_map.set_tile_size(64, 32);
		staggered_map.set_map_size(6, 8);
		let center = staggered_map.cell_to_map_center(1, 1);
		assert_eq!(staggered_map.cell_to_map_position(1, 1), Vector2::create(96.0, 16.0));
		assert_eq!(center, Vector2::create(128.0, 32.0));
		assert_eq!(staggered_map.map_position_to_cell(center.x, center.y), GridCell::create(1, 1));
		assert_eq!(staggered_map.map_position_to_cell(1.0, 1.0), GridCell::create(-1, -1));
		assert_eq!(staggered_map.map_position_to_cell(63.0, 31.0), GridCell::create(0, 1));
		assert_eq!(staggered_map.read_map_pixel_size(), Vector2::create(416.0, 144.0));
	}

	//==============================================================================
	/// 덩이 거르기와 그리는 칸 차례. (원본 culling, cullingIndividual, 화면 [0, 320] x [0, 180])
	//==============================================================================
	#[test]
	fn chunk_culling() {
		let image = Image::create_from_pixels(144, 54, vec![0; 144 * 54 * 4]);
		let mut draw_map = Tilemap::new();
		draw_map.set_tile_size(16, 16);
		draw_map.add_tileset(Tileset::create_grid(image, 16, 16, 1, 2), 0);
		let layer = draw_map.create_layer("ground");
		for row in 0..4 {
			for column in 0..4 {
				layer.set_tile(column, row, 1);
			}
		}
		layer.set_tile(100, 100, 2);
		layer.set_tile(-40, 3, 2);
		let layout = draw_map.create_layout();
		let draw_margin = draw_map.read_draw_margin(&layout);
		let layer = draw_map.get_layer(0).expect("layer");
		let mut is_screen_visible = |rect: Rect| rect.position.x + rect.size.x >= 0.0 && rect.position.y + rect.size.y >= 0.0 && rect.position.x <= 320.0 && rect.position.y <= 180.0;
		let mut visible_chunk_list = Tilemap::select_visible_chunks(&layout, layer, draw_margin, &mut is_screen_visible);
		assert_eq!((layer.get_chunk_count(), visible_chunk_list.len()), (3, 1));
		let mut draw_cell_list = Vec::new();
		draw_map.collect_draw_cells(&layout, layer.get_chunk_size(), &mut visible_chunk_list, &mut draw_cell_list);
		assert_eq!(draw_cell_list.len() * 6, 96);
		assert_eq!(draw_cell_list[0], (0, 0, 1));
		assert_eq!(draw_cell_list[4], (0, 1, 1));
	}

	//==============================================================================
	/// 시계 방향 90 도 칸의 쿼드. (원본 rotatedQuad)
	//==============================================================================
	#[test]
	fn rotated_quad() {
		let mut tileset = Tileset::new();
		tileset.set_tile_size(16, 16);
		tileset.set_margin(1);
		tileset.set_spacing(2);
		tileset.fit_grid_to_image_size(144, 54);
		let mut tilemap = Tilemap::new();
		tilemap.set_tile_size(16, 16);
		let layout = tilemap.create_layout();
		let rotated_tile = Tilemap::rotate_tile(11, 1);
		let placement = Tilemap::read_tile_placement(&layout, &tileset, 10, Tilemap::read_tile_flags(rotated_tile), 2, 1).expect("placement");
		let quad_values = placement.write_quad(144.0, 54.0);
		let expected_values = [48.0, 16.0, 37.0 / 144.0, 19.0 / 54.0, 48.0, 32.0, 53.0 / 144.0, 19.0 / 54.0, 32.0, 16.0, 37.0 / 144.0, 35.0 / 54.0];
		for (actual_value, expected_value) in quad_values.iter().zip(expected_values.iter()) {
			assert_near(*actual_value, *expected_value);
		}
	}

	//==============================================================================
	/// 충돌 모양: 돌린 사각형, 뒤집은 다각형과 타원, 층 전체, 점. (원본 collision*)
	//==============================================================================
	#[test]
	fn collision_shapes() {
		let mut collision_tileset = Tileset::new();
		collision_tileset.set_tile_size(16, 16);
		collision_tileset.set_columns(8);
		collision_tileset.set_tile_count(24);
		collision_tileset.set_tile_collision_shapes(11, &[TileCollisionShape::create_rectangle(0.0, 5.0, 16.0, 7.0)]);
		collision_tileset.set_tile_collision_shapes(12, &[TileCollisionShape::create_polygon(0.0, 16.0, &[Vector2::create(0.0, 0.0), Vector2::create(16.0, -16.0), Vector2::create(16.0, 0.0)])]);
		let mut ellipse_shape = TileCollisionShape::create_ellipse(2.0, 3.0, 12.0, 6.0);
		ellipse_shape.rotation = 30.0;
		collision_tileset.set_tile_collision_shapes(9, &[ellipse_shape]);
		let mut collision_map = Tilemap::new();
		collision_map.set_tile_size(16, 16);
		collision_map.add_tileset(collision_tileset, 0);
		let layer = collision_map.create_layer("walls");
		layer.set_offset(Vector2::create(4.0, -2.0));
		layer.set_tile(2, 3, Tilemap::rotate_tile(12, 1));
		layer.set_tile(5, 0, 13 | TileFlag::HORIZONTAL_FLIP);
		layer.set_tile(7, 1, 10 | TileFlag::VERTICAL_FLIP);
		let rotated_list = collision_map.read_cell_collision_shapes(0, 2, 3);
		assert_eq!(rotated_list.len(), 1);
		assert_shape(&rotated_list[0], TileCollisionShapeKind::Rectangle, [40.0, 46.0, 7.0, 16.0, 0.0, 0.0], &[]);
		let polygon_list = collision_map.read_cell_collision_shapes(0, 5, 0);
		assert_shape(&polygon_list[0], TileCollisionShapeKind::Polygon, [100.0, 14.0, 0.0, 0.0, 0.0, 3.0], &[(0.0, 0.0), (-16.0, -16.0), (-16.0, 0.0)]);
		let ellipse_list = collision_map.read_cell_collision_shapes(0, 7, 1);
		assert_shape(&ellipse_list[0], TileCollisionShapeKind::Ellipse, [128.3923, 21.0, 12.0, 6.0, 150.0, 0.0], &[]);
		assert_eq!(collision_map.read_layer_collision_shapes(0).len(), 3);
		let mut point_matrix = TransformMatrix::identity();
		point_matrix.translate(10.0, 20.0);
		point_matrix.scale(2.0, 3.0);
		assert_shape(&TileCollisionShape::create_point(3.0, 4.0).create_transformed(&point_matrix), TileCollisionShapeKind::Point, [16.0, 32.0, 0.0, 0.0, 0.0, 0.0], &[]);
	}
}
