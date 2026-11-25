//==============================================================================
//! 방향 포커스 내비게이터.
//! - 원본 misc/focusnavigator.js 를 옮긴 것이다. 게임패드 십자키 / 키보드 화살표로 UI 항목 사이를 옮겨 다닐 때,
//!   "이동 방향 앞쪽에 있으면서 축에서 벗어난 만큼 벌점을 준 거리" 가 가장 가까운 항목으로 포커스를 옮긴다. (콘솔 UI 내비게이션의 표준 방식)
//! - 항목은 (id, rect) 로 등록한다. 그리는 방식과 상관없는 순수 좌표 알고리즘이다.
//! - 원본과 다른 점:
//!   id 는 String, null 은 None 이다. options 객체는 FocusNavigatorOptions 구조체다.
//==============================================================================
use crate::base::rect::Rect;

//==============================================================================
/// 내비게이터 설정. (원본 options = { cross_axis_penalty = 2.5 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusNavigatorOptions {
	/// 이동 축에서 벗어난 거리에 곱하는 벌점.
	pub cross_axis_penalty: f32,
}

impl Default for FocusNavigatorOptions {
	//==============================================================================
	/// 기본값. (벌점 2.5)
	//==============================================================================
	fn default() -> FocusNavigatorOptions {
		FocusNavigatorOptions {
			cross_axis_penalty: 2.5,
		}
	}
}

//==============================================================================
/// 등록한 항목 하나. (원본 { id, rect })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
struct FocusItem {
	id: String,
	rect: Rect,
}

//==============================================================================
/// 방향 포커스 내비게이터.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FocusNavigator {
	item_list: Vec<FocusItem>,
	focused_id: Option<String>,
	cross_axis_penalty: f32,
}

impl FocusNavigator {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub fn new(options: FocusNavigatorOptions) -> FocusNavigator {
		FocusNavigator {
			item_list: Vec::new(),
			focused_id: None,
			cross_axis_penalty: options.cross_axis_penalty,
		}
	}

	//==============================================================================
	/// 항목 등록.
	//==============================================================================
	pub fn add_item(&mut self, id: &str, rect: Rect) {
		self.item_list.push(FocusItem {
			id: id.to_string(),
			rect,
		});
	}

	//==============================================================================
	/// 항목 모두 빼기. (포커스도 비운다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.item_list.clear();
		self.focused_id = None;
	}

	//==============================================================================
	/// 포커스 정하기.
	//==============================================================================
	pub fn set_focused_id(&mut self, id: Option<&str>) {
		self.focused_id = id.map(|id| id.to_string());
	}

	//==============================================================================
	/// 지금 포커스 반환.
	//==============================================================================
	pub fn get_focused_id(&self) -> Option<&str> {
		self.focused_id.as_deref()
	}

	//==============================================================================
	/// 방향 이동. (direction_x / direction_y 는 -1, 0, 1. 예: 오른쪽 = (1, 0))
	/// - 포커스가 없으면 방향과 상관없이 가장 왼쪽 위 항목을 고른다.
	/// - 포커스한 항목이 목록에 없으면 첫 항목을 고른다.
	/// - 옮길 곳이 없으면 포커스를 그대로 두고 None 을 돌려준다.
	//==============================================================================
	pub fn move_focus(&mut self, direction_x: f32, direction_y: f32) -> Option<String> {
		let first_list_item = self.item_list.first()?;
		let focused_id = match &self.focused_id {
			Some(focused_id) => {
				focused_id.clone()
			}
			None => {
				let mut first_item = first_list_item;
				for item in self.item_list.iter() {
					let item_score = item.rect.position.y * 10000.0 + item.rect.position.x;
					let first_score = first_item.rect.position.y * 10000.0 + first_item.rect.position.x;
					if item_score < first_score {
						first_item = item;
					}
				}
				let first_id = first_item.id.clone();
				self.focused_id = Some(first_id.clone());
				return Some(first_id);
			}
		};

		let current_item = match self.item_list.iter().find(|item| item.id == focused_id) {
			Some(current_item) => {
				current_item
			}
			None => {
				let first_id = first_list_item.id.clone();
				self.focused_id = Some(first_id.clone());
				return Some(first_id);
			}
		};
		let current_center_x = current_item.rect.position.x + current_item.rect.size.x * 0.5;
		let current_center_y = current_item.rect.position.y + current_item.rect.size.y * 0.5;

		let mut best_item: Option<&FocusItem> = None;
		let mut best_score = f32::INFINITY;
		for item in self.item_list.iter() {
			if item.id == focused_id {
				continue;
			}
			let item_center_x = item.rect.position.x + item.rect.size.x * 0.5;
			let item_center_y = item.rect.position.y + item.rect.size.y * 0.5;
			let delta_x = item_center_x - current_center_x;
			let delta_y = item_center_y - current_center_y;

			// 이동 방향 앞쪽에 있는 항목만 후보로 삼는다.
			let forward_distance = delta_x * direction_x + delta_y * direction_y;
			if forward_distance <= 0.0 {
				continue;
			}
			let cross_distance = (delta_x * direction_y).abs() + (delta_y * direction_x).abs();
			let score = forward_distance + cross_distance * self.cross_axis_penalty;
			if score < best_score {
				best_score = score;
				best_item = Some(item);
			}
		}
		let best_id = best_item?.id.clone();
		self.focused_id = Some(best_id.clone());
		Some(best_id)
	}
}

impl Default for FocusNavigator {
	//==============================================================================
	/// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> FocusNavigator {
		FocusNavigator::new(FocusNavigatorOptions::default())
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 2x2 격자와 멀리 떨어진 대각선 항목에서 방향 이동.
	//==============================================================================
	#[test]
	fn move_focus_picks_nearest_along_direction() {
		let mut focus_navigator = FocusNavigator::default();
		assert_eq!(focus_navigator.move_focus(1.0, 0.0), None);
		focus_navigator.add_item("bottomRight", Rect::create(200.0, 100.0, 80.0, 40.0));
		focus_navigator.add_item("topLeft", Rect::create(0.0, 0.0, 80.0, 40.0));
		focus_navigator.add_item("topRight", Rect::create(200.0, 0.0, 80.0, 40.0));
		focus_navigator.add_item("bottomLeft", Rect::create(0.0, 100.0, 80.0, 40.0));

		// 포커스가 없으면 왼쪽 위.
		assert_eq!(focus_navigator.move_focus(0.0, 1.0), Some(String::from("topLeft")));
		assert_eq!(focus_navigator.move_focus(1.0, 0.0), Some(String::from("topRight")));
		assert_eq!(focus_navigator.move_focus(0.0, 1.0), Some(String::from("bottomRight")));
		assert_eq!(focus_navigator.move_focus(-1.0, 0.0), Some(String::from("bottomLeft")));

		// 앞쪽에 아무것도 없으면 그대로.
		assert_eq!(focus_navigator.move_focus(-1.0, 0.0), None);
		assert_eq!(focus_navigator.get_focused_id(), Some("bottomLeft"));

		// 축에서 벗어난 거리에 벌점: 위로 갈 때 바로 위(거리 100)가 오른쪽 위 대각선보다 낫다.
		assert_eq!(focus_navigator.move_focus(0.0, -1.0), Some(String::from("topLeft")));

		// 목록에 없는 포커스면 첫 항목.
		focus_navigator.set_focused_id(Some("missing"));
		assert_eq!(focus_navigator.move_focus(1.0, 0.0), Some(String::from("bottomRight")));
		focus_navigator.clear();
		assert_eq!(focus_navigator.get_focused_id(), None);
	}
}
