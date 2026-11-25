//==============================================================================
// 식별자. (원본 base/identifier.js)
// - create_instance_id: 원본 Object 의 instance_id 자리. (스레드와 상관없이 겹치지 않는 번호)
// - Identifier: 원본처럼 처음 번호와 늘릴 수로 차례 번호를 낸다. 원본 reset 은 자기 값을 자기에 다시 넣어 아무것도 하지 않는데(원본 버그),
//   러스트 판은 처음 번호로 되돌린다.
// - 원본 Enum(begin, auto, readonly)은 자바스크립트 객체로 열거형을 흉내 낸 것이라 옮기지 않는다. 러스트는 enum 을 쓴다.
//==============================================================================
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
// 새 인스턴스 식별자. (1 부터, 겹치지 않음)
//==============================================================================
pub fn create_instance_id() -> u64 {
	NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

//==============================================================================
// 차례 번호 발급기. (원본 Identifier)
//==============================================================================
#[derive(Clone, Debug)]
pub struct Identifier {
	initial_serial_number: i64,
	serial_number: i64,
	increase_number: i64,
}

impl Identifier {
	//==============================================================================
	// 생성. (처음 번호, 한 번에 늘릴 수, 원본 기본값은 0 과 1)
	//==============================================================================
	pub fn new(initial_serial_number: i64, initial_increase_number: i64) -> Identifier {
		Identifier {
			initial_serial_number,
			serial_number: initial_serial_number,
			increase_number: initial_increase_number,
		}
	}

	//==============================================================================
	// 처음 번호로 되돌리기.
	//==============================================================================
	pub fn reset(&mut self) {
		self.serial_number = self.initial_serial_number;
	}

	//==============================================================================
	// 다음 번호. (increase_number 가 0 이 아니면 그만큼, 0 이면 만들 때 정한 수만큼 늘린 뒤 돌려준다)
	//==============================================================================
	pub fn auto(&mut self, increase_number: i64) -> i64 {
		if increase_number != 0 {
			self.serial_number += increase_number;
		}
		else {
			self.serial_number += self.increase_number;
		}
		self.serial_number
	}
}

impl Default for Identifier {
	fn default() -> Identifier {
		Identifier::new(0, 1)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 차례 번호는 정한 수만큼 늘고, 되돌리면 처음 번호부터 다시 센다.
	//==============================================================================
	#[test]
	fn identifier_counts_and_resets() {
		let mut identifier = Identifier::default();
		assert_eq!(identifier.auto(0), 1);
		assert_eq!(identifier.auto(0), 2);
		assert_eq!(identifier.auto(10), 12);
		identifier.reset();
		assert_eq!(identifier.auto(0), 1);
		let mut stepped_identifier = Identifier::new(100, 5);
		assert_eq!(stepped_identifier.auto(0), 105);
		assert!(create_instance_id() < create_instance_id());
	}
}
