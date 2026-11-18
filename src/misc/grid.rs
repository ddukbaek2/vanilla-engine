//==============================================================================
// 그리드 유틸.
// - 원본 misc/grid.js 를 옮긴 것이다. 연결성 검사(플러드필)와 타일 충돌 판정처럼 격자 게임마다 다시 만들던 것을 모았다.
// - 칸의 통과 여부는 isPassableHandler(column, row) 콜백으로 정한다. 모두 정적 함수다.
// - 원본과 다른 점:
//   원본의 { column, row } 객체는 GridCell, toWorldCenter 의 { x, y } 는 Vector2 로 돌려준다.
//   칸 번호는 음수 칸(월드 왼쪽 위 바깥)도 나타내도록 i32 다.
//   좌표는 f32 로 받지만 칸 계산은 원본(JS number)처럼 f64 로 한다. (testRectOverlap 의 0.000001 빼기가 f32 에서는 사라진다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;

//==============================================================================
// 칸 좌표. (원본 { column, row })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GridCell {
	pub column: i32,
	pub row: i32,
}

impl GridCell {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub const fn create(column: i32, row: i32) -> GridCell {
		GridCell {
			column,
			row,
		}
	}
}

// 상하좌우 이웃 칸으로 가는 차이. (원본과 같은 순서: 오른쪽, 왼쪽, 아래, 위)
pub(crate) const NEIGHBOR_OFFSETS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

//==============================================================================
// 그리드 유틸. (정적 함수 묶음)
//==============================================================================
pub struct Grid;

impl Grid {
	//==============================================================================
	// 플러드필. (시작 칸에서 상하좌우로 이어지는 통과 가능한 칸들을 모두 방문한 순서대로 돌려준다)
	//==============================================================================
	pub fn floodFill(columnCount: i32, rowCount: i32, start: GridCell, mut isPassableHandler: impl FnMut(i32, i32) -> bool) -> Vec<GridCell> {
		let mut visitedList: Vec<GridCell> = Vec::new();
		if start.column < 0 || start.column >= columnCount || start.row < 0 || start.row >= rowCount {
			return visitedList;
		}
		if !isPassableHandler(start.column, start.row) {
			return visitedList;
		}
		let mut isVisited = vec![false; (columnCount * rowCount) as usize];
		let mut pendingStack: Vec<GridCell> = vec![start];
		isVisited[(start.row * columnCount + start.column) as usize] = true;
		while let Some(cell) = pendingStack.pop() {
			visitedList.push(cell);
			for (offsetColumn, offsetRow) in NEIGHBOR_OFFSETS {
				let nextColumn = cell.column + offsetColumn;
				let nextRow = cell.row + offsetRow;
				if nextColumn < 0 || nextColumn >= columnCount || nextRow < 0 || nextRow >= rowCount {
					continue;
				}
				let nextIndex = (nextRow * columnCount + nextColumn) as usize;
				if isVisited[nextIndex] {
					continue;
				}
				if !isPassableHandler(nextColumn, nextRow) {
					continue;
				}
				isVisited[nextIndex] = true;
				pendingStack.push(GridCell::create(nextColumn, nextRow));
			}
		}
		visitedList
	}

	//==============================================================================
	// 연결성 검사. (시작 칸에서 닿는 칸 수가 expectedCount 와 같은지. 예: 모든 부품이 코어와 붙어 있는지)
	//==============================================================================
	pub fn isAllConnected(columnCount: i32, rowCount: i32, start: GridCell, isPassableHandler: impl FnMut(i32, i32) -> bool, expectedCount: usize) -> bool {
		let visitedList = Grid::floodFill(columnCount, rowCount, start, isPassableHandler);
		visitedList.len() == expectedCount
	}

	//==============================================================================
	// 사각형-타일 충돌 검사. (월드 사각형이 걸치는 타일을 돌며 막힌 타일과 겹치면 true)
	// - 이동 판정에서 "이동 뒤 자리가 장애물과 겹치면 취소" 하는 데 쓴다.
	//==============================================================================
	pub fn testRectOverlap(worldRect: Rect, tileSize: f32, mut isBlockedHandler: impl FnMut(i32, i32) -> bool) -> bool {
		// 원본(JS number)처럼 f64 로 셈한다. (f32 로는 큰 좌표에서 0.000001 을 빼도 값이 그대로라 다음 칸까지 걸친다)
		let rectX = worldRect.position.x as f64;
		let rectY = worldRect.position.y as f64;
		let rectWidth = worldRect.size.x as f64;
		let rectHeight = worldRect.size.y as f64;
		let tileSize = tileSize as f64;
		let startColumn = (rectX / tileSize).floor() as i32;
		let endColumn = ((rectX + rectWidth - 0.000001) / tileSize).floor() as i32;
		let startRow = (rectY / tileSize).floor() as i32;
		let endRow = ((rectY + rectHeight - 0.000001) / tileSize).floor() as i32;
		for row in startRow..=endRow {
			for column in startColumn..=endColumn {
				if isBlockedHandler(column, row) {
					return true;
				}
			}
		}
		false
	}

	//==============================================================================
	// 월드 좌표 → 타일 좌표.
	//==============================================================================
	pub fn toCell(worldX: f32, worldY: f32, tileSize: f32) -> GridCell {
		let column = (worldX as f64 / tileSize as f64).floor() as i32;
		let row = (worldY as f64 / tileSize as f64).floor() as i32;
		GridCell::create(column, row)
	}

	//==============================================================================
	// 타일 좌표 → 타일 가운데 월드 좌표.
	//==============================================================================
	pub fn toWorldCenter(column: i32, row: i32, tileSize: f32) -> Vector2 {
		let worldX = (column as f32 + 0.5) * tileSize;
		let worldY = (row as f32 + 0.5) * tileSize;
		Vector2::create(worldX, worldY)
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	// 시험용 지도. ('#' 이 막힌 칸)
	const TEST_MAP: [&str; 4] = [
		"..#..",
		"..#..",
		"###..",
		".....",
	];

	//==============================================================================
	// 시험용 지도의 칸이 막혔는지.
	//==============================================================================
	fn isBlocked(column: i32, row: i32) -> bool {
		if column < 0 || row < 0 || row as usize >= TEST_MAP.len() {
			return true;
		}
		let rowText = TEST_MAP[row as usize].as_bytes();
		if column as usize >= rowText.len() {
			return true;
		}
		rowText[column as usize] == b'#'
	}

	//==============================================================================
	// 플러드필은 벽으로 막힌 구역만 돌고, 막힌 시작 칸이나 밖이면 비어 있다.
	//==============================================================================
	#[test]
	fn floodFillStopsAtWalls() {
		let topLeftArea = Grid::floodFill(5, 4, GridCell::create(0, 0), |column, row| !isBlocked(column, row));
		assert_eq!(topLeftArea.len(), 4);
		assert_eq!(topLeftArea[0], GridCell::create(0, 0));
		let rightArea = Grid::floodFill(5, 4, GridCell::create(4, 0), |column, row| !isBlocked(column, row));
		assert_eq!(rightArea.len(), 11);
		assert!(Grid::isAllConnected(5, 4, GridCell::create(4, 3), |column, row| !isBlocked(column, row), 11));
		assert!(Grid::floodFill(5, 4, GridCell::create(2, 0), |column, row| !isBlocked(column, row)).is_empty());
		assert!(Grid::floodFill(5, 4, GridCell::create(5, 0), |_, _| true).is_empty());
	}

	//==============================================================================
	// 칸 번호 계산과 사각형 겹침.
	//==============================================================================
	#[test]
	fn cellMathAndRectOverlap() {
		assert_eq!(Grid::toCell(70.0, 31.9, 32.0), GridCell::create(2, 0));
		assert_eq!(Grid::toCell(-1.0, 0.0, 32.0), GridCell::create(-1, 0));
		assert_eq!(Grid::toWorldCenter(2, 1, 32.0), Vector2::create(80.0, 48.0));

		// 오른쪽 끝이 정확히 타일 경계면 그 다음 타일은 걸치지 않는다.
		let mut visitedCells: Vec<GridCell> = Vec::new();
		let isOverlapped = Grid::testRectOverlap(Rect::create(0.0, 0.0, 64.0, 32.0), 32.0, |column, row| {
			visitedCells.push(GridCell::create(column, row));
			false
		});
		assert!(!isOverlapped);
		assert_eq!(visitedCells, vec![GridCell::create(0, 0), GridCell::create(1, 0)]);
		assert!(Grid::testRectOverlap(Rect::create(40.0, 10.0, 40.0, 10.0), 32.0, isBlocked));
		assert!(!Grid::testRectOverlap(Rect::create(0.0, 0.0, 60.0, 60.0), 32.0, isBlocked));
	}
}
