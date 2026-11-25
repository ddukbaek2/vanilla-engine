//==============================================================================
//! 식별자. (원본 base/identifier.js)
//! - create_instance_id: 원본 Object 의 instance_id 자리. (스레드와 상관없이 겹치지 않는 번호)
//! - Identifier: 원본처럼 처음 번호와 늘릴 수로 차례 번호를 낸다. reset 은 처음 번호로 되돌린다. (원본 reset 이 자기 값을 자기에 다시 넣어
//!   아무것도 하지 않던 버그는 원본에서도 고쳤다)
//! - 원본 Enum(begin, auto, readonly)은 자바스크립트 객체로 열거형을 흉내 낸 것이라 옮기지 않는다. 러스트는 enum 을 쓴다.
//! - create_guid: 원본 base/reflection.js 의 createGUID. (앞 12 자리는 밀리초 시각, 나머지는 난수인 UUID 꼴 글. 원본 reflection.js 의 나머지는 자바스크립트 객체 반사라 옮기지 않는다)
//==============================================================================
use crate::base::math;
use std::sync::atomic::{AtomicU64, Ordering};
use web_time::{SystemTime, UNIX_EPOCH};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
/// 새 인스턴스 식별자. (1 부터, 겹치지 않음)
//==============================================================================
pub fn create_instance_id() -> u64 {
	NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

//==============================================================================
/// 고유식별자 생성. (원본 createGUID, "0192a3f2-b1c4-4d8e-8f1a-0e5d4c3b2a19")
/// - 48 비트 밀리초 시각을 16 진수 12 자리로 앞에 두고, 판 4 와 변형 8 ~ b 자리 말고는 난수다. (74 비트)
//==============================================================================
pub fn create_guid() -> String {
	let milliseconds = SystemTime::now().duration_since(UNIX_EPOCH).map(|duration| duration.as_millis() as u64).unwrap_or(0);
	let time_text = format!("{:012x}", milliseconds & 0xFFFF_FFFF_FFFF);
	let random_digit = || -> char {
		std::char::from_digit((math::random() * 16.0) as u32 & 15, 16).unwrap_or('0')
	};
	let variant_digit = std::char::from_digit(((math::random() * 4.0) as u32 & 3) | 8, 16).unwrap_or('8');
	let mut guid = String::with_capacity(36);
	guid.push_str(&time_text[0..8]);
	guid.push('-');
	guid.push_str(&time_text[8..12]);
	guid.push_str("-4");
	for _ in 0..3 {
		guid.push(random_digit());
	}
	guid.push('-');
	guid.push(variant_digit);
	for _ in 0..3 {
		guid.push(random_digit());
	}
	guid.push('-');
	for _ in 0..12 {
		guid.push(random_digit());
	}
	guid
}

//==============================================================================
/// 차례 번호 발급기. (원본 Identifier)
//==============================================================================
#[derive(Clone, Debug)]
pub struct Identifier {
	initial_serial_number: i64,
	serial_number: i64,
	increase_number: i64,
}

impl Identifier {
	//==============================================================================
	/// 생성. (처음 번호, 한 번에 늘릴 수, 원본 기본값은 0 과 1)
	//==============================================================================
	pub fn new(initial_serial_number: i64, initial_increase_number: i64) -> Identifier {
		Identifier {
			initial_serial_number,
			serial_number: initial_serial_number,
			increase_number: initial_increase_number,
		}
	}

	//==============================================================================
	/// 처음 번호로 되돌리기.
	//==============================================================================
	pub fn reset(&mut self) {
		self.serial_number = self.initial_serial_number;
	}

	//==============================================================================
	/// 다음 번호. (increase_number 가 0 이 아니면 그만큼, 0 이면 만들 때 정한 수만큼 늘린 뒤 돌려준다)
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
	/// 차례 번호는 정한 수만큼 늘고, 되돌리면 처음 번호부터 다시 센다.
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

	//==============================================================================
	/// 고유식별자는 8-4-4-4-12 꼴의 16 진수이고, 판 자리는 4, 변형 자리는 8 ~ b 다.
	//==============================================================================
	#[test]
	fn guid_has_uuid_layout() {
		let guid = create_guid();
		let part_list: Vec<&str> = guid.split('-').collect();
		assert_eq!(part_list.iter().map(|part| part.len()).collect::<Vec<usize>>(), vec![8, 4, 4, 4, 12]);
		assert!(guid.chars().all(|character| character == '-' || character.is_ascii_hexdigit()));
		assert!(part_list[2].starts_with('4'));
		assert!(matches!(part_list[3].chars().next(), Some('8' | '9' | 'a' | 'b')));
		assert_ne!(create_guid(), create_guid());
	}
}
