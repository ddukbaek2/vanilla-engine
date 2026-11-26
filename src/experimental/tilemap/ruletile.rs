//==============================================================================
//! 자동 타일. (룰 타일, 원본 src/experimental/tilemap/ruletile.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 유니티 2D Tilemap Extras 의 RuleTile 과 같은 규칙이다. 규칙마다 둘레 8 칸의 조건(상관없음, 같은 자동 타일, 다른 것)과 놓을 타일 값을 두고,
//!   위에서부터 처음 맞는 규칙의 타일을 놓는다. 맞는 것이 없으면 기본 타일이다.
//! - 규칙의 변환: 고정, 돌리기(조건을 시계 방향 90 도씩 돌려 보고 맞으면 타일도 그만큼 돌린다), 좌우 거울, 상하 거울, 둘 다.
//!   차례는 유니티와 같다. (고정을 먼저, 돌리기는 90, 180, 270, 둘 다 거울은 좌우상하, 좌우, 상하)
//! - 칸에는 타일 값만 든다. 어느 칸이 이 자동 타일인지는 gid 가 기본 타일이나 규칙 타일 가운데 하나인지로 본다.
//! - 둘레는 정사각 격자의 8 칸이다. (아이소는 같은 칸 셈이라 그대로 쓴다, 육각 둘레는 범위 밖)
//==============================================================================
use crate::experimental::tilemap::tilemap::Tilemap;
use crate::experimental::tilemap::tilemaplayer::TilemapLayer;

//==============================================================================
/// 둘레 칸 조건. (유니티 RuleTile.TilingRuleOutput.Neighbor)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleTileNeighbor {
	/// 상관없음.
	Any,
	/// 같은 자동 타일이어야 한다.
	This,
	/// 같은 자동 타일이 아니어야 한다. (빈 칸 포함)
	NotThis,
}

//==============================================================================
/// 규칙 변환. (유니티 RuleTile.TilingRuleOutput.Transform)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleTileTransform {
	/// 그대로만 본다.
	Fixed,
	/// 시계 방향 90 도씩 돌려 본다.
	Rotated,
	/// 좌우 거울로도 본다.
	MirrorX,
	/// 상하 거울로도 본다.
	MirrorY,
	/// 좌우, 상하, 둘 다 거울로도 본다.
	MirrorXY,
}

/// 둘레 8 칸의 열, 행 차이. (왼쪽 위, 위, 오른쪽 위, 왼쪽, 오른쪽, 왼쪽 아래, 아래, 오른쪽 아래, 화면 아래가 + 행)
const NEIGHBOR_OFFSETS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

//==============================================================================
/// 자동 타일 규칙 하나.
///
/// # 예제
/// 오른쪽에 같은 타일이 있을 때의 규칙
/// ```rust
/// use vanilla::experimental::tilemap::{RuleTileNeighbor, RuleTileRule, RuleTileTransform};
///
/// let any = RuleTileNeighbor::Any;
/// let rule = RuleTileRule::new(&[any, any, any, any, RuleTileNeighbor::This, any, any, any], 7, RuleTileTransform::MirrorX);
/// assert_eq!((rule.neighbors.len(), rule.tile, rule.transform), (8, 7, RuleTileTransform::MirrorX));
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct RuleTileRule {
	/// 둘레 8 칸 조건. (왼쪽 위, 위, 오른쪽 위, 왼쪽, 오른쪽, 왼쪽 아래, 아래, 오른쪽 아래)
	pub neighbors: [RuleTileNeighbor; 8],
	/// 맞으면 놓을 타일 값. (뒤집기 비트를 포함한 gid)
	pub tile: u32,
	/// 변환.
	pub transform: RuleTileTransform,
}

impl RuleTileRule {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `neighbors`: 둘레 8 칸 조건. (모자란 칸은 상관없음)
	/// - `tile`: 맞으면 놓을 타일 값.
	/// - `transform`: 변환.
	///
	/// # 돌려줌
	/// 새 규칙.
	//==============================================================================
	pub fn new(neighbors: &[RuleTileNeighbor], tile: u32, transform: RuleTileTransform) -> RuleTileRule {
		let mut neighbor_array = [RuleTileNeighbor::Any; 8];
		for (neighbor_index, neighbor) in neighbors.iter().take(8).enumerate() {
			neighbor_array[neighbor_index] = *neighbor;
		}
		RuleTileRule {
			neighbors: neighbor_array,
			tile,
			transform,
		}
	}
}

//==============================================================================
/// 자동 타일.
///
/// # 예제
/// 길 자동 타일 (끝, 곧은 길, 모퉁이, 세 갈래, 네 갈래)
/// ```rust
/// use vanilla::experimental::tilemap::{RuleTile, RuleTileNeighbor, RuleTileTransform, TileFlag, TilemapLayer};
///
/// let any = RuleTileNeighbor::Any;
/// let same = RuleTileNeighbor::This;
/// let other = RuleTileNeighbor::NotThis;
/// let mut road_tile = RuleTile::new(20);
/// road_tile.add_rule(&[any, same, any, same, same, any, same, any], 25, RuleTileTransform::Fixed);
/// road_tile.add_rule(&[any, other, any, same, same, any, same, any], 24, RuleTileTransform::Rotated);
/// road_tile.add_rule(&[any, other, any, same, same, any, other, any], 22, RuleTileTransform::Rotated);
/// road_tile.add_rule(&[any, other, any, other, same, any, same, any], 23, RuleTileTransform::Rotated);
/// road_tile.add_rule(&[any, other, any, other, same, any, other, any], 21, RuleTileTransform::Rotated);
///
/// let mut layer = TilemapLayer::new("road");
/// road_tile.paint(&mut layer, 0, 0);
/// road_tile.paint(&mut layer, 1, 0);
/// road_tile.paint(&mut layer, 2, 0);
/// road_tile.paint(&mut layer, 1, 1);
/// assert_eq!(layer.get_tile(0, 0), 21);
/// assert_eq!(layer.get_tile(1, 0), 24);
/// assert_eq!(layer.get_tile(2, 0), 21 | TileFlag::HORIZONTAL_FLIP | TileFlag::VERTICAL_FLIP);
/// assert_eq!(layer.get_tile(1, 1), 21 | TileFlag::DIAGONAL_FLIP | TileFlag::VERTICAL_FLIP);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct RuleTile {
	default_tile: u32,
	rules: Vec<RuleTileRule>,
}

impl RuleTile {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `default_tile`: 맞는 규칙이 없을 때 놓을 타일 값. (gid)
	///
	/// # 돌려줌
	/// 규칙 없는 자동 타일.
	//==============================================================================
	pub fn new(default_tile: u32) -> RuleTile {
		RuleTile {
			default_tile,
			rules: Vec::new(),
		}
	}

	//==============================================================================
	/// 기본 타일 설정.
	///
	/// # 인자
	/// - `default_tile`: 타일 값.
	//==============================================================================
	pub fn set_default_tile(&mut self, default_tile: u32) {
		self.default_tile = default_tile;
	}

	//==============================================================================
	/// 기본 타일 반환.
	///
	/// # 돌려줌
	/// 타일 값.
	//==============================================================================
	pub fn get_default_tile(&self) -> u32 {
		self.default_tile
	}

	//==============================================================================
	/// 규칙 더하기. (맨 뒤에, 앞의 규칙이 먼저 맞는다)
	///
	/// # 인자
	/// - `neighbors`: 둘레 8 칸 조건.
	/// - `tile`: 맞으면 놓을 타일 값.
	/// - `transform`: 변환.
	///
	/// # 돌려줌
	/// 더한 규칙.
	//==============================================================================
	pub fn add_rule(&mut self, neighbors: &[RuleTileNeighbor], tile: u32, transform: RuleTileTransform) -> &mut RuleTileRule {
		self.rules.push(RuleTileRule::new(neighbors, tile, transform));
		let rule_index = self.rules.len() - 1;
		&mut self.rules[rule_index]
	}

	//==============================================================================
	/// 규칙 목록 반환.
	///
	/// # 돌려줌
	/// 규칙. (위에서부터 보는 차례)
	//==============================================================================
	pub fn get_rules(&self) -> &[RuleTileRule] {
		&self.rules
	}

	//==============================================================================
	/// 타일 값이 이 자동 타일인지. (gid 가 기본 타일이나 규칙 타일 가운데 하나면 true)
	///
	/// # 인자
	/// - `tile`: 타일 값. (뒤집기 비트는 보지 않는다)
	///
	/// # 돌려줌
	/// 이 자동 타일이면 true.
	//==============================================================================
	pub fn is_member(&self, tile: u32) -> bool {
		let tile_id = Tilemap::read_tile_id(tile);
		if tile_id == 0 {
			return false;
		}
		if tile_id == Tilemap::read_tile_id(self.default_tile) {
			return true;
		}
		self.rules.iter().any(|rule| Tilemap::read_tile_id(rule.tile) == tile_id)
	}

	//==============================================================================
	/// 둘레에 맞는 타일 값 셈하기.
	///
	/// # 인자
	/// - `is_this_handler`: (column, row) 칸이 이 자동 타일이면 true 를 돌려주는 함수.
	/// - `column`: 열.
	/// - `row`: 행.
	///
	/// # 돌려줌
	/// 놓을 타일 값. (규칙의 변환을 건 값, 맞는 것이 없으면 기본 타일)
	///
	/// # 예제
	/// 둘레를 직접 물어 고를 타일 셈하기
	/// ```rust
	/// use vanilla::experimental::tilemap::{RuleTile, RuleTileNeighbor, RuleTileTransform, TileFlag};
	///
	/// let any = RuleTileNeighbor::Any;
	/// let mut wall_tile = RuleTile::new(1);
	/// wall_tile.add_rule(&[any, any, any, any, RuleTileNeighbor::This, any, any, any], 2, RuleTileTransform::MirrorX);
	/// let is_wall = |column: i32, row: i32| row == 0 && (0..=1).contains(&column);
	/// assert_eq!(wall_tile.resolve_tile(&is_wall, 0, 0), 2);
	/// assert_eq!(wall_tile.resolve_tile(&is_wall, 1, 0), 2 | TileFlag::HORIZONTAL_FLIP);
	/// ```
	//==============================================================================
	pub fn resolve_tile(&self, is_this_handler: &dyn Fn(i32, i32) -> bool, column: i32, row: i32) -> u32 {
		for rule in self.rules.iter() {
			if RuleTile::match_rule(rule, is_this_handler, column, row, 0, false, false) {
				return rule.tile;
			}
			match rule.transform {
				RuleTileTransform::Rotated => {
					for turn_count in 1..4 {
						if RuleTile::match_rule(rule, is_this_handler, column, row, turn_count, false, false) {
							return Tilemap::rotate_tile(rule.tile, turn_count);
						}
					}
				}
				RuleTileTransform::MirrorXY => {
					if RuleTile::match_rule(rule, is_this_handler, column, row, 0, true, true) {
						return Tilemap::flip_tile_vertically(Tilemap::flip_tile_horizontally(rule.tile));
					}
					if RuleTile::match_rule(rule, is_this_handler, column, row, 0, true, false) {
						return Tilemap::flip_tile_horizontally(rule.tile);
					}
					if RuleTile::match_rule(rule, is_this_handler, column, row, 0, false, true) {
						return Tilemap::flip_tile_vertically(rule.tile);
					}
				}
				RuleTileTransform::MirrorX => {
					if RuleTile::match_rule(rule, is_this_handler, column, row, 0, true, false) {
						return Tilemap::flip_tile_horizontally(rule.tile);
					}
				}
				RuleTileTransform::MirrorY => {
					if RuleTile::match_rule(rule, is_this_handler, column, row, 0, false, true) {
						return Tilemap::flip_tile_vertically(rule.tile);
					}
				}
				RuleTileTransform::Fixed => {
				}
			}
		}
		self.default_tile
	}

	//==============================================================================
	/// 규칙 하나가 둘레에 맞는지. (조건을 시계 방향으로 돌리거나 거울로 비춘 자리에서 본다)
	//==============================================================================
	fn match_rule(rule: &RuleTileRule, is_this_handler: &dyn Fn(i32, i32) -> bool, column: i32, row: i32, clockwise_quarter_turn_count: i32, is_mirror_x: bool, is_mirror_y: bool) -> bool {
		for (neighbor_index, neighbor) in rule.neighbors.iter().enumerate() {
			if *neighbor == RuleTileNeighbor::Any {
				continue;
			}
			let (mut offset_column, mut offset_row) = NEIGHBOR_OFFSETS[neighbor_index];
			for _ in 0..clockwise_quarter_turn_count {
				// 화면 기준 시계 방향 90 도. (오른쪽 → 아래)
				let turned_column = -offset_row;
				offset_row = offset_column;
				offset_column = turned_column;
			}
			if is_mirror_x {
				offset_column = -offset_column;
			}
			if is_mirror_y {
				offset_row = -offset_row;
			}
			let is_this = is_this_handler(column + offset_column, row + offset_row);
			if *neighbor == RuleTileNeighbor::This && !is_this {
				return false;
			}
			if *neighbor == RuleTileNeighbor::NotThis && is_this {
				return false;
			}
		}
		true
	}

	//==============================================================================
	/// 층의 칸 하나를 둘레에 맞게 다시 고르기. (이 자동 타일인 칸만 고친다)
	///
	/// # 인자
	/// - `layer`: 층.
	/// - `column`: 열.
	/// - `row`: 행.
	//==============================================================================
	pub fn refresh(&self, layer: &mut TilemapLayer, column: i32, row: i32) {
		let tile = layer.get_tile(column, row);
		if !self.is_member(tile) {
			return;
		}
		let resolved_tile = {
			let layer_view: &TilemapLayer = layer;
			let is_this_handler = |neighbor_column: i32, neighbor_row: i32| self.is_member(layer_view.get_tile(neighbor_column, neighbor_row));
			self.resolve_tile(&is_this_handler, column, row)
		};
		layer.set_tile(column, row, resolved_tile);
	}

	//==============================================================================
	/// 층의 칸에 이 자동 타일 칠하기. (그 칸과 둘레 8 칸을 다시 고른다)
	///
	/// # 인자
	/// - `layer`: 층.
	/// - `column`: 열.
	/// - `row`: 행.
	//==============================================================================
	pub fn paint(&self, layer: &mut TilemapLayer, column: i32, row: i32) {
		layer.set_tile(column, row, self.default_tile);
		self.refresh_around(layer, column, row);
	}

	//==============================================================================
	/// 층의 칸 지우기. (둘레 8 칸을 다시 고른다)
	///
	/// # 인자
	/// - `layer`: 층.
	/// - `column`: 열.
	/// - `row`: 행.
	//==============================================================================
	pub fn erase(&self, layer: &mut TilemapLayer, column: i32, row: i32) {
		layer.set_tile(column, row, 0);
		self.refresh_around(layer, column, row);
	}

	//==============================================================================
	/// 칸과 둘레 8 칸 다시 고르기.
	///
	/// # 인자
	/// - `layer`: 층.
	/// - `column`: 열.
	/// - `row`: 행.
	//==============================================================================
	pub fn refresh_around(&self, layer: &mut TilemapLayer, column: i32, row: i32) {
		for row_offset in -1..=1 {
			for column_offset in -1..=1 {
				self.refresh(layer, column + column_offset, row + row_offset);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::experimental::tilemap::tilemap::TileFlag;

	//==============================================================================
	/// 길 자동 타일 칠하기와 지우기, 좌우 거울. (원본 check.mjs 의 ruleTile, ruleTileErase, ruleTileMirror)
	//==============================================================================
	#[test]
	fn road_rule_tile() {
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
		let cell_list = [(0, 0), (1, 0), (2, 0), (1, 1), (1, 2), (3, 3)];
		for (column, row) in cell_list.iter() {
			road_tile.paint(&mut road_layer, *column, *row);
		}
		let tile_list: Vec<u32> = cell_list.iter().map(|(column, row)| road_layer.get_tile(*column, *row)).collect();
		assert_eq!(tile_list, vec![18, 21, 18 | TileFlag::HORIZONTAL_FLIP | TileFlag::VERTICAL_FLIP, 19 | TileFlag::DIAGONAL_FLIP | TileFlag::HORIZONTAL_FLIP, 18 | TileFlag::DIAGONAL_FLIP | TileFlag::VERTICAL_FLIP, 17]);
		road_tile.erase(&mut road_layer, 1, 1);
		assert_eq!((road_layer.get_tile(1, 0), road_layer.get_tile(1, 2)), (19, 17));

		let mut mirror_tile = RuleTile::new(1);
		mirror_tile.add_rule(&[any, any, any, any, same, any, any, any], 2, RuleTileTransform::MirrorX);
		let is_wall = |column: i32, row: i32| row == 0 && (0..=1).contains(&column);
		assert_eq!((mirror_tile.resolve_tile(&is_wall, 0, 0), mirror_tile.resolve_tile(&is_wall, 1, 0), mirror_tile.resolve_tile(&is_wall, 5, 5)), (2, 2 | TileFlag::HORIZONTAL_FLIP, 1));
	}
}
