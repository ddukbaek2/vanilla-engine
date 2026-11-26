//==============================================================================
//! 그리드 길찾기. (A*)
//! - 원본 misc/pathfinder.js 를 옮긴 것이다. 4방향 이동, 맨해튼 거리 휴리스틱, 이진 최소 힙으로 열린 목록을 관리한다.
//! - is_blocked_handler(column, row) 가 참이면 막힌 칸이다.
//! - 결과는 시작 칸을 빼고 도착 칸까지의 칸 목록. 길이 없으면 None.
//! - 원본과 다른 점:
//!   { column, row } 는 GridCell, null 은 None 이다.
//!   힙은 원본 코드를 그대로 옮겨서, 비용이 같은 길이 여럿일 때 원본과 같은 길을 고른다. (표준 BinaryHeap 은 고르는 순서가 다르다)
//==============================================================================
use crate::misc::grid::{GridCell, NEIGHBOR_OFFSETS};

//==============================================================================
/// 열린 목록. ([예상 총비용, 칸 번호] 쌍의 이진 최소 힙, 원본 heap_scores / heap_indices)
//==============================================================================
struct OpenHeap {
	heap_scores: Vec<f64>,
	heap_indices: Vec<usize>,
}

impl OpenHeap {
	//==============================================================================
	/// 생성.
	//==============================================================================
	fn new() -> OpenHeap {
		OpenHeap {
			heap_scores: Vec::new(),
			heap_indices: Vec::new(),
		}
	}

	//==============================================================================
	/// 비었는지.
	//==============================================================================
	fn is_empty(&self) -> bool {
		self.heap_scores.is_empty()
	}

	//==============================================================================
	/// 넣기. (맨 뒤에 넣고 부모보다 작으면 올린다)
	//==============================================================================
	fn push(&mut self, score: f64, cell_index: usize) {
		self.heap_scores.push(score);
		self.heap_indices.push(cell_index);
		let mut child_position = self.heap_scores.len() - 1;
		while child_position > 0 {
			let parent_position = (child_position - 1) >> 1;
			if self.heap_scores[parent_position] <= self.heap_scores[child_position] {
				break;
			}
			self.heap_scores.swap(parent_position, child_position);
			self.heap_indices.swap(parent_position, child_position);
			child_position = parent_position;
		}
	}

	//==============================================================================
	/// 가장 작은 것 꺼내기. (맨 뒤 것을 위로 올리고 자식보다 크면 내린다)
	//==============================================================================
	fn pop(&mut self) -> Option<usize> {
		let top_index = *self.heap_indices.first()?;
		let last_score = self.heap_scores.pop()?;
		let last_index = self.heap_indices.pop()?;
		if !self.heap_scores.is_empty() {
			self.heap_scores[0] = last_score;
			self.heap_indices[0] = last_index;
			let heap_length = self.heap_scores.len();
			let mut parent_position = 0;
			loop {
				let left_position = parent_position * 2 + 1;
				let right_position = left_position + 1;
				let mut smallest_position = parent_position;
				if left_position < heap_length && self.heap_scores[left_position] < self.heap_scores[smallest_position] {
					smallest_position = left_position;
				}
				if right_position < heap_length && self.heap_scores[right_position] < self.heap_scores[smallest_position] {
					smallest_position = right_position;
				}
				if smallest_position == parent_position {
					break;
				}
				self.heap_scores.swap(parent_position, smallest_position);
				self.heap_indices.swap(parent_position, smallest_position);
				parent_position = smallest_position;
			}
		}
		Some(top_index)
	}
}

//==============================================================================
/// 그리드 길찾기. (정적 함수 묶음)
/// - 상태가 없다. find_path 를 바로 부른다.
///
/// # 예제
/// 벽을 돌아가는 길
/// ```rust
/// use vanilla::{GridCell, PathFinder};
///
/// let map = [
/// 	"....",
/// 	".##.",
/// 	".###",
/// ];
/// let is_wall = |column: i32, row: i32| -> bool {
/// 	map[row as usize].as_bytes()[column as usize] == b'#'
/// };
/// let path = PathFinder::find_path(4, 3, GridCell::create(0, 1), GridCell::create(3, 1), is_wall).unwrap();
/// assert_eq!(path.len(), 5);
/// assert_eq!(path[0], GridCell::create(0, 0));
/// assert_eq!(*path.last().unwrap(), GridCell::create(3, 1));
/// ```
//==============================================================================
pub struct PathFinder;

impl PathFinder {
	//==============================================================================
	/// 경로 찾기. (시작이나 도착이 밖이거나 도착이 막혔으면 None, 시작과 도착이 같으면 빈 목록)
	/// - 4방향으로 한 칸에 비용 1 씩 움직이는 최단 경로. 시작 칸이 막혔는지는 보지 않는다.
	///
	/// # 인자
	/// - `column_count`: 그리드 열 수. (칸 번호는 0 ~ column_count - 1)
	/// - `row_count`: 그리드 행 수. (칸 번호는 0 ~ row_count - 1)
	/// - `start`: 시작 칸.
	/// - `goal`: 도착 칸.
	/// - `is_blocked_handler`: (열, 행) 칸이 막혔으면 true 를 돌려주는 콜백. 그리드 안의 칸에만 부른다.
	///
	/// # 돌려줌
	/// 시작 칸을 빼고 도착 칸까지 이웃끼리 이어진 칸 목록. 길이 없으면 None.
	///
	/// # 예제
	/// 막힌 곳이 없으면 곧은 길, 도착이 막히면 None
	/// ```rust
	/// use vanilla::{GridCell, PathFinder};
	///
	/// let straight_path = PathFinder::find_path(3, 1, GridCell::create(0, 0), GridCell::create(2, 0), |_, _| false);
	/// assert_eq!(straight_path, Some(vec![GridCell::create(1, 0), GridCell::create(2, 0)]));
	///
	/// let blocked_path = PathFinder::find_path(3, 1, GridCell::create(0, 0), GridCell::create(2, 0), |column, _| column == 2);
	/// assert!(blocked_path.is_none());
	/// ```
	//==============================================================================
	pub fn find_path(column_count: i32, row_count: i32, start: GridCell, goal: GridCell, mut is_blocked_handler: impl FnMut(i32, i32) -> bool) -> Option<Vec<GridCell>> {
		let is_inside = |column: i32, row: i32| -> bool {
			column >= 0 && column < column_count && row >= 0 && row < row_count
		};
		if !is_inside(start.column, start.row) || !is_inside(goal.column, goal.row) {
			return None;
		}
		if is_blocked_handler(goal.column, goal.row) {
			return None;
		}
		if start == goal {
			return Some(Vec::new());
		}

		let cell_count = (column_count * row_count) as usize;
		let to_index = |column: i32, row: i32| -> usize {
			(row * column_count + column) as usize
		};
		let to_cell = |cell_index: usize| -> GridCell {
			let column = (cell_index % column_count as usize) as i32;
			let row = (cell_index / column_count as usize) as i32;
			GridCell::create(column, row)
		};
		let heuristic = |column: i32, row: i32| -> f64 {
			((column - goal.column).abs() + (row - goal.row).abs()) as f64
		};
		let mut cost_from_start = vec![f64::INFINITY; cell_count];
		let mut came_from: Vec<Option<usize>> = vec![None; cell_count];
		let mut is_closed = vec![false; cell_count];
		let mut open_heap = OpenHeap::new();

		let start_index = to_index(start.column, start.row);
		let goal_index = to_index(goal.column, goal.row);
		cost_from_start[start_index] = 0.0;
		open_heap.push(heuristic(start.column, start.row), start_index);

		while !open_heap.is_empty() {
			let current_index = match open_heap.pop() {
				Some(current_index) => {
					current_index
				}
				None => {
					break;
				}
			};
			if current_index == goal_index {
				// 경로 거꾸로 따라가기.
				let mut path: Vec<GridCell> = Vec::new();
				let mut trace_index = goal_index;
				while trace_index != start_index {
					path.push(to_cell(trace_index));
					trace_index = match came_from[trace_index] {
						Some(previous_index) => {
							previous_index
						}
						None => {
							break;
						}
					};
				}
				path.reverse();
				return Some(path);
			}
			if is_closed[current_index] {
				continue;
			}
			is_closed[current_index] = true;

			let current_cell = to_cell(current_index);
			for (offset_column, offset_row) in NEIGHBOR_OFFSETS {
				let next_column = current_cell.column + offset_column;
				let next_row = current_cell.row + offset_row;
				if !is_inside(next_column, next_row) {
					continue;
				}
				if is_blocked_handler(next_column, next_row) {
					continue;
				}
				let next_index = to_index(next_column, next_row);
				if is_closed[next_index] {
					continue;
				}
				let next_cost = cost_from_start[current_index] + 1.0;
				if next_cost < cost_from_start[next_index] {
					cost_from_start[next_index] = next_cost;
					came_from[next_index] = Some(current_index);
					open_heap.push(next_cost + heuristic(next_column, next_row), next_index);
				}
			}
		}
		None
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 글 지도의 칸이 막혔는지. ('#' 이 벽)
	//==============================================================================
	fn is_wall(map: &[&str], column: i32, row: i32) -> bool {
		map[row as usize].as_bytes()[column as usize] == b'#'
	}

	//==============================================================================
	/// 벽을 돌아가는 최단 경로. (시작 칸은 빼고 이웃 칸끼리 이어진다)
	//==============================================================================
	#[test]
	fn finds_shortest_path_around_wall() {
		let map = [
			".....",
			".###.",
			"...#.",
			".#...",
		];
		let start = GridCell::create(0, 2);
		let goal = GridCell::create(4, 2);
		let path = PathFinder::find_path(5, 4, start, goal, |column, row| is_wall(&map, column, row)).expect("path");
		assert_eq!(path.len(), 6);
		assert_eq!(*path.last().expect("goal"), goal);
		let mut previous_cell = start;
		for cell in path.iter() {
			let step_length = (cell.column - previous_cell.column).abs() + (cell.row - previous_cell.row).abs();
			assert_eq!(step_length, 1);
			assert!(!is_wall(&map, cell.column, cell.row));
			previous_cell = *cell;
		}
	}

	//==============================================================================
	/// 길이 없거나, 도착이 막혔거나, 밖이거나, 같은 칸.
	//==============================================================================
	#[test]
	fn edge_cases() {
		let closed_map = [
			"..#..",
			"..#..",
		];
		assert!(PathFinder::find_path(5, 2, GridCell::create(0, 0), GridCell::create(4, 0), |column, row| is_wall(&closed_map, column, row)).is_none());
		assert!(PathFinder::find_path(5, 2, GridCell::create(0, 0), GridCell::create(2, 0), |column, row| is_wall(&closed_map, column, row)).is_none());
		assert!(PathFinder::find_path(5, 2, GridCell::create(0, 0), GridCell::create(5, 0), |_, _| false).is_none());
		assert_eq!(PathFinder::find_path(5, 2, GridCell::create(1, 1), GridCell::create(1, 1), |_, _| false), Some(Vec::new()));
		let straight_path = PathFinder::find_path(3, 1, GridCell::create(0, 0), GridCell::create(2, 0), |_, _| false).expect("path");
		assert_eq!(straight_path, vec![GridCell::create(1, 0), GridCell::create(2, 0)]);
	}
}
