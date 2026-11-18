//==============================================================================
// 그리드 길찾기. (A*)
// - 원본 misc/pathfinder.js 를 옮긴 것이다. 4방향 이동, 맨해튼 거리 휴리스틱, 이진 최소 힙으로 열린 목록을 관리한다.
// - isBlockedHandler(column, row) 가 참이면 막힌 칸이다.
// - 결과는 시작 칸을 빼고 도착 칸까지의 칸 목록. 길이 없으면 None.
// - 원본과 다른 점:
//   { column, row } 는 GridCell, null 은 None 이다.
//   힙은 원본 코드를 그대로 옮겨서, 비용이 같은 길이 여럿일 때 원본과 같은 길을 고른다. (표준 BinaryHeap 은 고르는 순서가 다르다)
//==============================================================================
use crate::misc::grid::{GridCell, NEIGHBOR_OFFSETS};

//==============================================================================
// 열린 목록. ([예상 총비용, 칸 번호] 쌍의 이진 최소 힙, 원본 heapScores / heapIndices)
//==============================================================================
struct OpenHeap {
	heapScores: Vec<f64>,
	heapIndices: Vec<usize>,
}

impl OpenHeap {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> OpenHeap {
		OpenHeap {
			heapScores: Vec::new(),
			heapIndices: Vec::new(),
		}
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	fn isEmpty(&self) -> bool {
		self.heapScores.is_empty()
	}

	//==============================================================================
	// 넣기. (맨 뒤에 넣고 부모보다 작으면 올린다)
	//==============================================================================
	fn push(&mut self, score: f64, cellIndex: usize) {
		self.heapScores.push(score);
		self.heapIndices.push(cellIndex);
		let mut childPosition = self.heapScores.len() - 1;
		while childPosition > 0 {
			let parentPosition = (childPosition - 1) >> 1;
			if self.heapScores[parentPosition] <= self.heapScores[childPosition] {
				break;
			}
			self.heapScores.swap(parentPosition, childPosition);
			self.heapIndices.swap(parentPosition, childPosition);
			childPosition = parentPosition;
		}
	}

	//==============================================================================
	// 가장 작은 것 꺼내기. (맨 뒤 것을 위로 올리고 자식보다 크면 내린다)
	//==============================================================================
	fn pop(&mut self) -> Option<usize> {
		let topIndex = *self.heapIndices.first()?;
		let lastScore = self.heapScores.pop()?;
		let lastIndex = self.heapIndices.pop()?;
		if !self.heapScores.is_empty() {
			self.heapScores[0] = lastScore;
			self.heapIndices[0] = lastIndex;
			let heapLength = self.heapScores.len();
			let mut parentPosition = 0;
			loop {
				let leftPosition = parentPosition * 2 + 1;
				let rightPosition = leftPosition + 1;
				let mut smallestPosition = parentPosition;
				if leftPosition < heapLength && self.heapScores[leftPosition] < self.heapScores[smallestPosition] {
					smallestPosition = leftPosition;
				}
				if rightPosition < heapLength && self.heapScores[rightPosition] < self.heapScores[smallestPosition] {
					smallestPosition = rightPosition;
				}
				if smallestPosition == parentPosition {
					break;
				}
				self.heapScores.swap(parentPosition, smallestPosition);
				self.heapIndices.swap(parentPosition, smallestPosition);
				parentPosition = smallestPosition;
			}
		}
		Some(topIndex)
	}
}

//==============================================================================
// 그리드 길찾기. (정적 함수 묶음)
//==============================================================================
pub struct PathFinder;

impl PathFinder {
	//==============================================================================
	// 경로 찾기. (시작이나 도착이 밖이거나 도착이 막혔으면 None, 시작과 도착이 같으면 빈 목록)
	//==============================================================================
	pub fn findPath(columnCount: i32, rowCount: i32, start: GridCell, goal: GridCell, mut isBlockedHandler: impl FnMut(i32, i32) -> bool) -> Option<Vec<GridCell>> {
		let isInside = |column: i32, row: i32| -> bool {
			column >= 0 && column < columnCount && row >= 0 && row < rowCount
		};
		if !isInside(start.column, start.row) || !isInside(goal.column, goal.row) {
			return None;
		}
		if isBlockedHandler(goal.column, goal.row) {
			return None;
		}
		if start == goal {
			return Some(Vec::new());
		}

		let cellCount = (columnCount * rowCount) as usize;
		let toIndex = |column: i32, row: i32| -> usize {
			(row * columnCount + column) as usize
		};
		let toCell = |cellIndex: usize| -> GridCell {
			let column = (cellIndex % columnCount as usize) as i32;
			let row = (cellIndex / columnCount as usize) as i32;
			GridCell::create(column, row)
		};
		let heuristic = |column: i32, row: i32| -> f64 {
			((column - goal.column).abs() + (row - goal.row).abs()) as f64
		};
		let mut costFromStart = vec![f64::INFINITY; cellCount];
		let mut cameFrom: Vec<Option<usize>> = vec![None; cellCount];
		let mut isClosed = vec![false; cellCount];
		let mut openHeap = OpenHeap::new();

		let startIndex = toIndex(start.column, start.row);
		let goalIndex = toIndex(goal.column, goal.row);
		costFromStart[startIndex] = 0.0;
		openHeap.push(heuristic(start.column, start.row), startIndex);

		while !openHeap.isEmpty() {
			let currentIndex = match openHeap.pop() {
				Some(currentIndex) => {
					currentIndex
				}
				None => {
					break;
				}
			};
			if currentIndex == goalIndex {
				// 경로 거꾸로 따라가기.
				let mut path: Vec<GridCell> = Vec::new();
				let mut traceIndex = goalIndex;
				while traceIndex != startIndex {
					path.push(toCell(traceIndex));
					traceIndex = match cameFrom[traceIndex] {
						Some(previousIndex) => {
							previousIndex
						}
						None => {
							break;
						}
					};
				}
				path.reverse();
				return Some(path);
			}
			if isClosed[currentIndex] {
				continue;
			}
			isClosed[currentIndex] = true;

			let currentCell = toCell(currentIndex);
			for (offsetColumn, offsetRow) in NEIGHBOR_OFFSETS {
				let nextColumn = currentCell.column + offsetColumn;
				let nextRow = currentCell.row + offsetRow;
				if !isInside(nextColumn, nextRow) {
					continue;
				}
				if isBlockedHandler(nextColumn, nextRow) {
					continue;
				}
				let nextIndex = toIndex(nextColumn, nextRow);
				if isClosed[nextIndex] {
					continue;
				}
				let nextCost = costFromStart[currentIndex] + 1.0;
				if nextCost < costFromStart[nextIndex] {
					costFromStart[nextIndex] = nextCost;
					cameFrom[nextIndex] = Some(currentIndex);
					openHeap.push(nextCost + heuristic(nextColumn, nextRow), nextIndex);
				}
			}
		}
		None
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 글 지도의 칸이 막혔는지. ('#' 이 벽)
	//==============================================================================
	fn isWall(map: &[&str], column: i32, row: i32) -> bool {
		map[row as usize].as_bytes()[column as usize] == b'#'
	}

	//==============================================================================
	// 벽을 돌아가는 최단 경로. (시작 칸은 빼고 이웃 칸끼리 이어진다)
	//==============================================================================
	#[test]
	fn findsShortestPathAroundWall() {
		let map = [
			".....",
			".###.",
			"...#.",
			".#...",
		];
		let start = GridCell::create(0, 2);
		let goal = GridCell::create(4, 2);
		let path = PathFinder::findPath(5, 4, start, goal, |column, row| isWall(&map, column, row)).expect("path");
		assert_eq!(path.len(), 6);
		assert_eq!(*path.last().expect("goal"), goal);
		let mut previousCell = start;
		for cell in path.iter() {
			let stepLength = (cell.column - previousCell.column).abs() + (cell.row - previousCell.row).abs();
			assert_eq!(stepLength, 1);
			assert!(!isWall(&map, cell.column, cell.row));
			previousCell = *cell;
		}
	}

	//==============================================================================
	// 길이 없거나, 도착이 막혔거나, 밖이거나, 같은 칸.
	//==============================================================================
	#[test]
	fn edgeCases() {
		let closedMap = [
			"..#..",
			"..#..",
		];
		assert!(PathFinder::findPath(5, 2, GridCell::create(0, 0), GridCell::create(4, 0), |column, row| isWall(&closedMap, column, row)).is_none());
		assert!(PathFinder::findPath(5, 2, GridCell::create(0, 0), GridCell::create(2, 0), |column, row| isWall(&closedMap, column, row)).is_none());
		assert!(PathFinder::findPath(5, 2, GridCell::create(0, 0), GridCell::create(5, 0), |_, _| false).is_none());
		assert_eq!(PathFinder::findPath(5, 2, GridCell::create(1, 1), GridCell::create(1, 1), |_, _| false), Some(Vec::new()));
		let straightPath = PathFinder::findPath(3, 1, GridCell::create(0, 0), GridCell::create(2, 0), |_, _| false).expect("path");
		assert_eq!(straightPath, vec![GridCell::create(1, 0), GridCell::create(2, 0)]);
	}
}
