//==============================================================================
// 방향 포커스 내비게이터.
// - 원본 misc/focusnavigator.js 를 옮긴 것이다. 게임패드 십자키 / 키보드 화살표로 UI 항목 사이를 옮겨 다닐 때,
//   "이동 방향 앞쪽에 있으면서 축에서 벗어난 만큼 벌점을 준 거리" 가 가장 가까운 항목으로 포커스를 옮긴다. (콘솔 UI 내비게이션의 표준 방식)
// - 항목은 (id, rect) 로 등록한다. 그리는 방식과 상관없는 순수 좌표 알고리즘이다.
// - 원본과 다른 점:
//   id 는 String, null 은 None 이다. options 객체는 FocusNavigatorOptions 구조체다.
//==============================================================================
use crate::base::rect::Rect;

//==============================================================================
// 내비게이터 설정. (원본 options = { crossAxisPenalty = 2.5 })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FocusNavigatorOptions {
	// 이동 축에서 벗어난 거리에 곱하는 벌점.
	pub crossAxisPenalty: f32,
}

impl Default for FocusNavigatorOptions {
	//==============================================================================
	// 기본값. (벌점 2.5)
	//==============================================================================
	fn default() -> FocusNavigatorOptions {
		FocusNavigatorOptions {
			crossAxisPenalty: 2.5,
		}
	}
}

//==============================================================================
// 등록한 항목 하나. (원본 { id, rect })
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
struct FocusItem {
	id: String,
	rect: Rect,
}

//==============================================================================
// 방향 포커스 내비게이터.
//==============================================================================
#[derive(Clone, Debug)]
pub struct FocusNavigator {
	itemList: Vec<FocusItem>,
	focusedId: Option<String>,
	crossAxisPenalty: f32,
}

impl FocusNavigator {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new(options: FocusNavigatorOptions) -> FocusNavigator {
		FocusNavigator {
			itemList: Vec::new(),
			focusedId: None,
			crossAxisPenalty: options.crossAxisPenalty,
		}
	}

	//==============================================================================
	// 항목 등록.
	//==============================================================================
	pub fn addItem(&mut self, id: &str, rect: Rect) {
		self.itemList.push(FocusItem {
			id: id.to_string(),
			rect,
		});
	}

	//==============================================================================
	// 항목 모두 빼기. (포커스도 비운다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.itemList.clear();
		self.focusedId = None;
	}

	//==============================================================================
	// 포커스 정하기.
	//==============================================================================
	pub fn setFocusedId(&mut self, id: Option<&str>) {
		self.focusedId = id.map(|id| id.to_string());
	}

	//==============================================================================
	// 지금 포커스 반환.
	//==============================================================================
	pub fn getFocusedId(&self) -> Option<&str> {
		self.focusedId.as_deref()
	}

	//==============================================================================
	// 방향 이동. (directionX / directionY 는 -1, 0, 1. 예: 오른쪽 = (1, 0))
	// - 포커스가 없으면 방향과 상관없이 가장 왼쪽 위 항목을 고른다.
	// - 포커스한 항목이 목록에 없으면 첫 항목을 고른다.
	// - 옮길 곳이 없으면 포커스를 그대로 두고 None 을 돌려준다.
	//==============================================================================
	pub fn moveFocus(&mut self, directionX: f32, directionY: f32) -> Option<String> {
		let firstListItem = self.itemList.first()?;
		let focusedId = match &self.focusedId {
			Some(focusedId) => {
				focusedId.clone()
			}
			None => {
				let mut firstItem = firstListItem;
				for item in self.itemList.iter() {
					let itemScore = item.rect.position.y * 10000.0 + item.rect.position.x;
					let firstScore = firstItem.rect.position.y * 10000.0 + firstItem.rect.position.x;
					if itemScore < firstScore {
						firstItem = item;
					}
				}
				let firstId = firstItem.id.clone();
				self.focusedId = Some(firstId.clone());
				return Some(firstId);
			}
		};

		let currentItem = match self.itemList.iter().find(|item| item.id == focusedId) {
			Some(currentItem) => {
				currentItem
			}
			None => {
				let firstId = firstListItem.id.clone();
				self.focusedId = Some(firstId.clone());
				return Some(firstId);
			}
		};
		let currentCenterX = currentItem.rect.position.x + currentItem.rect.size.x * 0.5;
		let currentCenterY = currentItem.rect.position.y + currentItem.rect.size.y * 0.5;

		let mut bestItem: Option<&FocusItem> = None;
		let mut bestScore = f32::INFINITY;
		for item in self.itemList.iter() {
			if item.id == focusedId {
				continue;
			}
			let itemCenterX = item.rect.position.x + item.rect.size.x * 0.5;
			let itemCenterY = item.rect.position.y + item.rect.size.y * 0.5;
			let deltaX = itemCenterX - currentCenterX;
			let deltaY = itemCenterY - currentCenterY;

			// 이동 방향 앞쪽에 있는 항목만 후보로 삼는다.
			let forwardDistance = deltaX * directionX + deltaY * directionY;
			if forwardDistance <= 0.0 {
				continue;
			}
			let crossDistance = (deltaX * directionY).abs() + (deltaY * directionX).abs();
			let score = forwardDistance + crossDistance * self.crossAxisPenalty;
			if score < bestScore {
				bestScore = score;
				bestItem = Some(item);
			}
		}
		let bestId = bestItem?.id.clone();
		self.focusedId = Some(bestId.clone());
		Some(bestId)
	}
}

impl Default for FocusNavigator {
	//==============================================================================
	// 기본값. (기본 설정으로 만든다)
	//==============================================================================
	fn default() -> FocusNavigator {
		FocusNavigator::new(FocusNavigatorOptions::default())
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 2x2 격자와 멀리 떨어진 대각선 항목에서 방향 이동.
	//==============================================================================
	#[test]
	fn moveFocusPicksNearestAlongDirection() {
		let mut focusNavigator = FocusNavigator::default();
		assert_eq!(focusNavigator.moveFocus(1.0, 0.0), None);
		focusNavigator.addItem("bottomRight", Rect::create(200.0, 100.0, 80.0, 40.0));
		focusNavigator.addItem("topLeft", Rect::create(0.0, 0.0, 80.0, 40.0));
		focusNavigator.addItem("topRight", Rect::create(200.0, 0.0, 80.0, 40.0));
		focusNavigator.addItem("bottomLeft", Rect::create(0.0, 100.0, 80.0, 40.0));

		// 포커스가 없으면 왼쪽 위.
		assert_eq!(focusNavigator.moveFocus(0.0, 1.0), Some(String::from("topLeft")));
		assert_eq!(focusNavigator.moveFocus(1.0, 0.0), Some(String::from("topRight")));
		assert_eq!(focusNavigator.moveFocus(0.0, 1.0), Some(String::from("bottomRight")));
		assert_eq!(focusNavigator.moveFocus(-1.0, 0.0), Some(String::from("bottomLeft")));

		// 앞쪽에 아무것도 없으면 그대로.
		assert_eq!(focusNavigator.moveFocus(-1.0, 0.0), None);
		assert_eq!(focusNavigator.getFocusedId(), Some("bottomLeft"));

		// 축에서 벗어난 거리에 벌점: 위로 갈 때 바로 위(거리 100)가 오른쪽 위 대각선보다 낫다.
		assert_eq!(focusNavigator.moveFocus(0.0, -1.0), Some(String::from("topLeft")));

		// 목록에 없는 포커스면 첫 항목.
		focusNavigator.setFocusedId(Some("missing"));
		assert_eq!(focusNavigator.moveFocus(1.0, 0.0), Some(String::from("bottomRight")));
		focusNavigator.clear();
		assert_eq!(focusNavigator.getFocusedId(), None);
	}
}
