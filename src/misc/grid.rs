//==============================================================================
//! 그리드 유틸.
//! - 원본 misc/grid.js 를 옮긴 것이다. 연결성 검사(플러드필)와 타일 충돌 판정처럼 격자 게임마다 다시 만들던 것을 모았다.
//! - 칸의 통과 여부는 is_passable_handler(column, row) 콜백으로 정한다. 모두 정적 함수다.
//! - 원본과 다른 점:
//!   원본의 { column, row } 객체는 GridCell, to_world_center 의 { x, y } 는 Vector2 로 돌려준다.
//!   칸 번호는 음수 칸(월드 왼쪽 위 바깥)도 나타내도록 i32 다.
//!   좌표는 f32 로 받지만 칸 계산은 원본(JS number)처럼 f64 로 한다. (test_rect_overlap 의 0.000001 빼기가 f32 에서는 사라진다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;

//==============================================================================
/// 칸 좌표. (원본 { column, row })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridCell {
	pub column: i32,
	pub row: i32,
}

impl GridCell {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub const fn create(column: i32, row: i32) -> GridCell {
		GridCell {
			column,
			row,
		}
	}
}

/// 상하좌우 이웃 칸으로 가는 차이. (원본과 같은 순서: 오른쪽, 왼쪽, 아래, 위)
pub(crate) const NEIGHBOR_OFFSETS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

//==============================================================================
/// 그리드 유틸. (정적 함수 묶음)
//==============================================================================
pub struct Grid;

impl Grid {
	//==============================================================================
	/// 플러드필. (시작 칸에서 상하좌우로 이어지는 통과 가능한 칸들을 모두 방문한 순서대로 돌려준다)
	//==============================================================================
	pub fn flood_fill(column_count: i32, row_count: i32, start: GridCell, mut is_passable_handler: impl FnMut(i32, i32) -> bool) -> Vec<GridCell> {
		let mut visited_list: Vec<GridCell> = Vec::new();
		if start.column < 0 || start.column >= column_count || start.row < 0 || start.row >= row_count {
			return visited_list;
		}
		if !is_passable_handler(start.column, start.row) {
			return visited_list;
		}
		let mut is_visited = vec![false; (column_count * row_count) as usize];
		let mut pending_stack: Vec<GridCell> = vec![start];
		is_visited[(start.row * column_count + start.column) as usize] = true;
		while let Some(cell) = pending_stack.pop() {
			visited_list.push(cell);
			for (offset_column, offset_row) in NEIGHBOR_OFFSETS {
				let next_column = cell.column + offset_column;
				let next_row = cell.row + offset_row;
				if next_column < 0 || next_column >= column_count || next_row < 0 || next_row >= row_count {
					continue;
				}
				let next_index = (next_row * column_count + next_column) as usize;
				if is_visited[next_index] {
					continue;
				}
				if !is_passable_handler(next_column, next_row) {
					continue;
				}
				is_visited[next_index] = true;
				pending_stack.push(GridCell::create(next_column, next_row));
			}
		}
		visited_list
	}

	//==============================================================================
	/// 연결성 검사. (시작 칸에서 닿는 칸 수가 expected_count 와 같은지. 예: 모든 부품이 코어와 붙어 있는지)
	//==============================================================================
	pub fn is_all_connected(column_count: i32, row_count: i32, start: GridCell, is_passable_handler: impl FnMut(i32, i32) -> bool, expected_count: usize) -> bool {
		let visited_list = Grid::flood_fill(column_count, row_count, start, is_passable_handler);
		visited_list.len() == expected_count
	}

	//==============================================================================
	/// 사각형-타일 충돌 검사. (월드 사각형이 걸치는 타일을 돌며 막힌 타일과 겹치면 true)
	/// - 이동 판정에서 "이동 뒤 자리가 장애물과 겹치면 취소" 하는 데 쓴다.
	//==============================================================================
	pub fn test_rect_overlap(world_rect: Rect, tile_size: f32, mut is_blocked_handler: impl FnMut(i32, i32) -> bool) -> bool {
		// 원본(JS number)처럼 f64 로 셈한다. (f32 로는 큰 좌표에서 0.000001 을 빼도 값이 그대로라 다음 칸까지 걸친다)
		let rect_x = world_rect.position.x as f64;
		let rect_y = world_rect.position.y as f64;
		let rect_width = world_rect.size.x as f64;
		let rect_height = world_rect.size.y as f64;
		let tile_size = tile_size as f64;
		let start_column = (rect_x / tile_size).floor() as i32;
		let end_column = ((rect_x + rect_width - 0.000001) / tile_size).floor() as i32;
		let start_row = (rect_y / tile_size).floor() as i32;
		let end_row = ((rect_y + rect_height - 0.000001) / tile_size).floor() as i32;
		for row in start_row..=end_row {
			for column in start_column..=end_column {
				if is_blocked_handler(column, row) {
					return true;
				}
			}
		}
		false
	}

	//==============================================================================
	/// 월드 좌표 → 타일 좌표.
	//==============================================================================
	pub fn to_cell(world_x: f32, world_y: f32, tile_size: f32) -> GridCell {
		let column = (world_x as f64 / tile_size as f64).floor() as i32;
		let row = (world_y as f64 / tile_size as f64).floor() as i32;
		GridCell::create(column, row)
	}

	//==============================================================================
	/// 타일 좌표 → 타일 가운데 월드 좌표.
	//==============================================================================
	pub fn to_world_center(column: i32, row: i32, tile_size: f32) -> Vector2 {
		let world_x = (column as f32 + 0.5) * tile_size;
		let world_y = (row as f32 + 0.5) * tile_size;
		Vector2::create(world_x, world_y)
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	/// 시험용 지도. ('#' 이 막힌 칸)
	const TEST_MAP: [&str; 4] = [
		"..#..",
		"..#..",
		"###..",
		".....",
	];

	//==============================================================================
	/// 시험용 지도의 칸이 막혔는지.
	//==============================================================================
	fn is_blocked(column: i32, row: i32) -> bool {
		if column < 0 || row < 0 || row as usize >= TEST_MAP.len() {
			return true;
		}
		let row_text = TEST_MAP[row as usize].as_bytes();
		if column as usize >= row_text.len() {
			return true;
		}
		row_text[column as usize] == b'#'
	}

	//==============================================================================
	/// 플러드필은 벽으로 막힌 구역만 돌고, 막힌 시작 칸이나 밖이면 비어 있다.
	//==============================================================================
	#[test]
	fn flood_fill_stops_at_walls() {
		let top_left_area = Grid::flood_fill(5, 4, GridCell::create(0, 0), |column, row| !is_blocked(column, row));
		assert_eq!(top_left_area.len(), 4);
		assert_eq!(top_left_area[0], GridCell::create(0, 0));
		let right_area = Grid::flood_fill(5, 4, GridCell::create(4, 0), |column, row| !is_blocked(column, row));
		assert_eq!(right_area.len(), 11);
		assert!(Grid::is_all_connected(5, 4, GridCell::create(4, 3), |column, row| !is_blocked(column, row), 11));
		assert!(Grid::flood_fill(5, 4, GridCell::create(2, 0), |column, row| !is_blocked(column, row)).is_empty());
		assert!(Grid::flood_fill(5, 4, GridCell::create(5, 0), |_, _| true).is_empty());
	}

	//==============================================================================
	/// 칸 번호 계산과 사각형 겹침.
	//==============================================================================
	#[test]
	fn cell_math_and_rect_overlap() {
		assert_eq!(Grid::to_cell(70.0, 31.9, 32.0), GridCell::create(2, 0));
		assert_eq!(Grid::to_cell(-1.0, 0.0, 32.0), GridCell::create(-1, 0));
		assert_eq!(Grid::to_world_center(2, 1, 32.0), Vector2::create(80.0, 48.0));

		// 오른쪽 끝이 정확히 타일 경계면 그 다음 타일은 걸치지 않는다.
		let mut visited_cells: Vec<GridCell> = Vec::new();
		let is_overlapped = Grid::test_rect_overlap(Rect::create(0.0, 0.0, 64.0, 32.0), 32.0, |column, row| {
			visited_cells.push(GridCell::create(column, row));
			false
		});
		assert!(!is_overlapped);
		assert_eq!(visited_cells, vec![GridCell::create(0, 0), GridCell::create(1, 0)]);
		assert!(Grid::test_rect_overlap(Rect::create(40.0, 10.0, 40.0, 10.0), 32.0, is_blocked));
		assert!(!Grid::test_rect_overlap(Rect::create(0.0, 0.0, 60.0, 60.0), 32.0, is_blocked));
	}
}
