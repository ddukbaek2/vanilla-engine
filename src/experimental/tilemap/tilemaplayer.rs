//==============================================================================
//! 타일맵 층. (원본 src/experimental/tilemap/tilemaplayer.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 칸마다 타일 값(32 비트, 위 4 비트는 뒤집기와 돌리기, 아래 28 비트는 지도 전체의 타일 번호 gid, 0 은 빈 칸)을 든다. (Tiled 의 전역 타일 번호와 같은 꼴)
//! - 칸은 덩이(chunk, 기본 16 x 16)로 나눠 든다. 쓴 덩이만 만들어 큰 맵과 끝없는 맵(음수 칸)도 담고, Tilemap 이 덩이마다 화면 밖을 거른다.
//! - 색(곱하기 틴트), 불투명도, 그리는 자리 오프셋, 섞는 방식, 속성을 층마다 둔다.
//! - 원본과 다른 점: 덩이는 (행, 열) 차례의 BTreeMap 에 든다. (원본은 Map 과 늘어놓은 목록) 섞는 방식은 BlendMode 다. (원본은 Canvas2D 이름 글)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::graphic::BlendMode;
use crate::experimental::physics2d::PhysicsVector2;
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// 덩이 한 변의 칸 수. (16 x 16 칸이면 쿼드 256 개, 꼭짓점 1536 개로 Graphic 한 번에 그릴 수 있다)
pub const DEFAULT_CHUNK_SIZE: i32 = 16;

//==============================================================================
/// 덩이 하나. (열과 행은 덩이 단위, 칸은 행 차례)
//==============================================================================
#[derive(Clone, Debug)]
pub(crate) struct TilemapChunk {
	pub(crate) column: i32,
	pub(crate) row: i32,
	pub(crate) tiles: Vec<u32>,
	pub(crate) tile_count: u32,
}

//==============================================================================
/// 타일맵 층.
///
/// # 예제
/// 칸 쓰고 읽기, 쓴 범위
/// ```rust
/// use vanilla::experimental::tilemap::TilemapLayer;
///
/// let mut layer = TilemapLayer::new("ground");
/// layer.set_tile(0, 0, 1);
/// layer.set_tile(-3, 40, 2);
/// layer.set_tiles(4, 4, 2, 2, &[5, 6, 7, 8]);
/// assert_eq!((layer.get_tile(5, 5), layer.get_tile(9, 9), layer.get_tile_count()), (8, 0, 6));
/// let cell_bounds = layer.read_cell_bounds();
/// assert_eq!((cell_bounds.position.x, cell_bounds.position.y, cell_bounds.size.x, cell_bounds.size.y), (-3.0, 0.0, 9.0, 41.0));
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct TilemapLayer {
	name: String,
	is_visible: bool,
	opacity: f64,
	color: Color,
	offset: PhysicsVector2,
	blend_mode: BlendMode,
	properties: Map<String, Value>,
	chunk_size: i32,
	chunk_table: BTreeMap<(i32, i32), TilemapChunk>,
}

impl TilemapLayer {
	//==============================================================================
	/// 생성. (덩이 한 변 16 칸)
	///
	/// # 인자
	/// - `name`: 이름.
	///
	/// # 돌려줌
	/// 빈 층.
	//==============================================================================
	pub fn new(name: &str) -> TilemapLayer {
		TilemapLayer::create_with_chunk_size(name, DEFAULT_CHUNK_SIZE)
	}

	//==============================================================================
	/// 덩이 크기를 정해 생성. (원본 생성자의 chunkSize 인자)
	///
	/// # 인자
	/// - `name`: 이름.
	/// - `chunk_size`: 덩이 한 변의 칸 수. (1 보다 작으면 1)
	///
	/// # 돌려줌
	/// 빈 층.
	//==============================================================================
	pub fn create_with_chunk_size(name: &str, chunk_size: i32) -> TilemapLayer {
		TilemapLayer {
			name: String::from(name),
			is_visible: true,
			opacity: 1.0,
			color: Color::white(),
			offset: PhysicsVector2::zero(),
			blend_mode: BlendMode::Normal,
			properties: Map::new(),
			chunk_size: chunk_size.max(1),
			chunk_table: BTreeMap::new(),
		}
	}

	//==============================================================================
	/// 이름 설정.
	///
	/// # 인자
	/// - `name`: 이름.
	//==============================================================================
	pub fn set_name(&mut self, name: &str) {
		self.name = String::from(name);
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 보이기 설정.
	///
	/// # 인자
	/// - `is_visible`: true 면 그린다.
	//==============================================================================
	pub fn set_visible(&mut self, is_visible: bool) {
		self.is_visible = is_visible;
	}

	//==============================================================================
	/// 보이는지 반환.
	///
	/// # 돌려줌
	/// 그리면 true. (처음은 true)
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.is_visible
	}

	//==============================================================================
	/// 불투명도 설정.
	///
	/// # 인자
	/// - `opacity`: 불투명도. (0 ~ 1, 색의 알파에 곱한다)
	//==============================================================================
	pub fn set_opacity(&mut self, opacity: f64) {
		self.opacity = opacity;
	}

	//==============================================================================
	/// 불투명도 반환.
	///
	/// # 돌려줌
	/// 불투명도. (처음은 1)
	//==============================================================================
	pub fn get_opacity(&self) -> f64 {
		self.opacity
	}

	//==============================================================================
	/// 곱하는 색 설정. (Tiled 의 tintcolor, 유니티 Tilemap.color, 언리얼 layer color)
	///
	/// # 인자
	/// - `color`: 색.
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	/// 곱하는 색 반환.
	///
	/// # 돌려줌
	/// 색. (처음은 흰색)
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 그리는 자리 오프셋 설정.
	///
	/// # 인자
	/// - `offset`: 오프셋. (픽셀)
	//==============================================================================
	pub fn set_offset(&mut self, offset: impl Into<PhysicsVector2>) {
		self.offset = offset.into();
	}

	//==============================================================================
	/// 그리는 자리 오프셋 반환.
	///
	/// # 돌려줌
	/// 오프셋. (픽셀, 처음은 0, 0)
	//==============================================================================
	pub fn get_offset(&self) -> PhysicsVector2 {
		self.offset
	}

	//==============================================================================
	/// 섞는 방식 설정.
	///
	/// # 인자
	/// - `blend_mode`: 섞는 방식. (처음은 Normal)
	//==============================================================================
	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		self.blend_mode = blend_mode;
	}

	//==============================================================================
	/// 섞는 방식 반환.
	///
	/// # 돌려줌
	/// 섞는 방식.
	//==============================================================================
	pub fn get_blend_mode(&self) -> BlendMode {
		self.blend_mode
	}

	//==============================================================================
	/// 층 속성 설정.
	///
	/// # 인자
	/// - `name`: 속성 이름.
	/// - `value`: 값.
	//==============================================================================
	pub fn set_property(&mut self, name: &str, value: Value) {
		self.properties.insert(String::from(name), value);
	}

	//==============================================================================
	/// 층 속성 반환.
	///
	/// # 인자
	/// - `name`: 속성 이름.
	///
	/// # 돌려줌
	/// 값. (없으면 None)
	//==============================================================================
	pub fn get_property(&self, name: &str) -> Option<&Value> {
		self.properties.get(name)
	}

	//==============================================================================
	/// 층 속성 모두 반환.
	///
	/// # 돌려줌
	/// 이름 → 값. (넣은 차례)
	//==============================================================================
	pub fn get_properties(&self) -> &Map<String, Value> {
		&self.properties
	}

	//==============================================================================
	/// 덩이 한 변의 칸 수 반환.
	///
	/// # 돌려줌
	/// 칸 수.
	//==============================================================================
	pub fn get_chunk_size(&self) -> i32 {
		self.chunk_size
	}

	//==============================================================================
	/// 칸에 타일 값 쓰기.
	///
	/// # 인자
	/// - `column`: 열. (음수도 된다)
	/// - `row`: 행. (음수도 된다)
	/// - `tile`: 타일 값. (뒤집기 비트를 포함한 gid, 0 이면 비운다)
	//==============================================================================
	pub fn set_tile(&mut self, column: i32, row: i32, tile: u32) {
		let chunk_size = self.chunk_size;
		let chunk_column = column.div_euclid(chunk_size);
		let chunk_row = row.div_euclid(chunk_size);
		let chunk_key = (chunk_row, chunk_column);
		if !self.chunk_table.contains_key(&chunk_key) {
			if tile == 0 {
				return;
			}
			self.chunk_table.insert(chunk_key, TilemapChunk {
				column: chunk_column,
				row: chunk_row,
				tiles: vec![0; (chunk_size * chunk_size) as usize],
				tile_count: 0,
			});
		}
		let is_empty = {
			let chunk = self.chunk_table.get_mut(&chunk_key).expect("chunk");
			let tile_index = ((row - chunk_row * chunk_size) * chunk_size + (column - chunk_column * chunk_size)) as usize;
			let previous_tile = chunk.tiles[tile_index];
			if previous_tile == 0 && tile != 0 {
				chunk.tile_count += 1;
			}
			else if previous_tile != 0 && tile == 0 {
				chunk.tile_count -= 1;
			}
			chunk.tiles[tile_index] = tile;
			chunk.tile_count == 0
		};
		if is_empty {
			self.chunk_table.remove(&chunk_key);
		}
	}

	//==============================================================================
	/// 칸의 타일 값 반환.
	///
	/// # 인자
	/// - `column`: 열.
	/// - `row`: 행.
	///
	/// # 돌려줌
	/// 타일 값. (뒤집기 비트를 포함한 gid, 빈 칸은 0)
	//==============================================================================
	pub fn get_tile(&self, column: i32, row: i32) -> u32 {
		let chunk_size = self.chunk_size;
		let chunk_column = column.div_euclid(chunk_size);
		let chunk_row = row.div_euclid(chunk_size);
		match self.chunk_table.get(&(chunk_row, chunk_column)) {
			Some(chunk) => {
				chunk.tiles[((row - chunk_row * chunk_size) * chunk_size + (column - chunk_column * chunk_size)) as usize]
			}
			None => {
				0
			}
		}
	}

	//==============================================================================
	/// 사각형 범위에 타일 값 한꺼번에 쓰기. (행 차례의 목록)
	///
	/// # 인자
	/// - `column`: 왼쪽 열.
	/// - `row`: 위 행.
	/// - `width`: 열 수.
	/// - `height`: 행 수.
	/// - `tile_list`: 타일 값. (width x height 개, 행마다 왼쪽에서 오른쪽, 모자라면 남은 칸은 그대로 둔다)
	//==============================================================================
	pub fn set_tiles(&mut self, column: i32, row: i32, width: i32, height: i32, tile_list: &[u32]) {
		for row_offset in 0..height {
			for column_offset in 0..width {
				let tile_index = (row_offset * width + column_offset) as usize;
				if tile_index >= tile_list.len() {
					return;
				}
				self.set_tile(column + column_offset, row + row_offset, tile_list[tile_index]);
			}
		}
	}

	//==============================================================================
	/// 모든 칸 비우기.
	//==============================================================================
	pub fn clear(&mut self) {
		self.chunk_table.clear();
	}

	//==============================================================================
	/// 빈 칸이 아닌 칸 수 반환.
	///
	/// # 돌려줌
	/// 칸 수.
	//==============================================================================
	pub fn get_tile_count(&self) -> u32 {
		self.chunk_table.values().map(|chunk| chunk.tile_count).sum()
	}

	//==============================================================================
	/// 빈 칸이 아닌 칸을 모두 감싸는 칸 범위 셈하기. (유니티 Tilemap.cellBounds 를 줄인 것)
	///
	/// # 돌려줌
	/// 칸 범위. (position 은 왼쪽 위 열과 행, size 는 열 수와 행 수, 비었으면 0 크기)
	//==============================================================================
	pub fn read_cell_bounds(&self) -> Rect {
		let chunk_size = self.chunk_size;
		let mut minimum_column = i32::MAX;
		let mut minimum_row = i32::MAX;
		let mut maximum_column = i32::MIN;
		let mut maximum_row = i32::MIN;
		for chunk in self.chunk_table.values() {
			for (tile_index, tile) in chunk.tiles.iter().enumerate() {
				if *tile == 0 {
					continue;
				}
				let column = chunk.column * chunk_size + tile_index as i32 % chunk_size;
				let row = chunk.row * chunk_size + tile_index as i32 / chunk_size;
				minimum_column = minimum_column.min(column);
				minimum_row = minimum_row.min(row);
				maximum_column = maximum_column.max(column);
				maximum_row = maximum_row.max(row);
			}
		}
		if minimum_column == i32::MAX {
			return Rect::zero();
		}
		Rect::create(minimum_column as f64, minimum_row as f64, (maximum_column - minimum_column + 1) as f64, (maximum_row - minimum_row + 1) as f64)
	}

	//==============================================================================
	/// 덩이 목록. (행, 열 차례, 빈 덩이는 없다)
	//==============================================================================
	pub(crate) fn get_chunks(&self) -> impl Iterator<Item = &TilemapChunk> {
		self.chunk_table.values()
	}

	//==============================================================================
	/// 덩이 수.
	//==============================================================================
	pub(crate) fn get_chunk_count(&self) -> usize {
		self.chunk_table.len()
	}
}
