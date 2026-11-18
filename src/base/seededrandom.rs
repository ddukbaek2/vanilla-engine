//==============================================================================
// 시드 난수 생성기. (원본 base/seededrandom.js, mulberry32)
// - 같은 시드에서 항상 같은 수열이 나오는 결정론적 난수다. 스테이지 생성, 리플레이, 대전 동기화처럼 재현이 필요한 곳에 쓴다.
// - 원본과 같은 시드면 같은 수열이 비트 단위로 똑같이 나온다. 원본의 >>> 0, Math.imul, ^ 는 32 비트 정수 연산이라
//   u32 의 wrapping_add / wrapping_mul / ^ 로 그대로 옮겼다. (중간의 mixed + imul(...) 덧셈도 ToInt32 를 거치면 2^32 나머지라
//   wrapping_add 와 같다) nextValue 는 원본처럼 f64 다. (32 비트 값을 2^32 로 나눈 것이라 f32 로는 끝자리가 뭉개진다)
// - 시드와 상태는 u32 다. 원본 (seed >>> 0) || 1 은 u32 시드 그대로, 0 이면 1 이다.
//   원본에서 음수나 2^32 넘는 수를 시드로 쓰던 곳은 그 수를 u32 로 바꿔 넘기면 같은 수열이 나온다. (as u32 는 ToUint32 와 같다)
// - nextRange 는 f32 를 받아 f32 를 돌려주되, 계산은 원본처럼 f64 로 하고 끝에 한 번만 f32 로 줄인다.
//   nextInt 는 i32 이고 계산은 원본처럼 f64 다.
// - pick 은 Option<&T>(원본 undefined 는 None), shuffle 은 슬라이스를 제자리에서 섞는다. (원본은 같은 배열을 돌려준다)
// - fromDate 는 원본이 Date 의 지역 시각 연, 월, 일을 읽는다. 러스트 std 에는 지역 시각이 없어서 연, 월(1 ~ 12), 일을 받는다.
//==============================================================================

// 날짜 시드에 섞는 기본 값. (원본 createDailySeed / fromDate 의 baseSeed 기본 인자)
pub const DAILY_BASE_SEED: u32 = 0x9E3779B1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeededRandom {
	state: u32,
}

impl Default for SeededRandom {
	//==============================================================================
	// 시드 1. (원본 기본 인자)
	//==============================================================================
	fn default() -> SeededRandom {
		SeededRandom::new(1)
	}
}

impl SeededRandom {
	//==============================================================================
	// 생성. (시드가 0 이면 1 을 쓴다)
	//==============================================================================
	pub fn new(seed: u32) -> SeededRandom {
		SeededRandom { state: normalizeSeed(seed) }
	}

	//==============================================================================
	// 시드 재설정. (시드가 0 이면 1 을 쓴다)
	//==============================================================================
	pub fn setSeed(&mut self, seed: u32) {
		self.state = normalizeSeed(seed);
	}

	//==============================================================================
	// 현재 내부 상태 반환. (저장, 복원용)
	//==============================================================================
	pub fn getState(&self) -> u32 {
		self.state
	}

	//==============================================================================
	// 내부 상태 복원. (0 도 그대로 받는다)
	//==============================================================================
	pub fn setState(&mut self, state: u32) {
		self.state = state;
	}

	//==============================================================================
	// 0 이상 1 미만의 난수를 돌려준다.
	//==============================================================================
	pub fn nextValue(&mut self) -> f64 {
		self.state = self.state.wrapping_add(0x6D2B79F5);
		let mut mixed = self.state;
		mixed = (mixed ^ (mixed >> 15)).wrapping_mul(mixed | 1);
		mixed ^= mixed.wrapping_add((mixed ^ (mixed >> 7)).wrapping_mul(mixed | 61));
		let result = mixed ^ (mixed >> 14);
		result as f64 / 4294967296.0
	}

	//==============================================================================
	// minValue 이상 maxValue 미만의 실수 난수를 돌려준다.
	//==============================================================================
	pub fn nextRange(&mut self, minValue: f32, maxValue: f32) -> f32 {
		let minimum = minValue as f64;
		let maximum = maxValue as f64;
		let value = minimum + (maximum - minimum) * self.nextValue();
		value as f32
	}

	//==============================================================================
	// minValue 이상 maxValue 이하의 정수 난수를 돌려준다.
	//==============================================================================
	pub fn nextInt(&mut self, minValue: i32, maxValue: i32) -> i32 {
		let minimum = minValue as f64;
		let span = maxValue as f64 - minimum + 1.0;
		let value = minimum + (self.nextValue() * span).floor();
		value as i32
	}

	//==============================================================================
	// 배열에서 하나 뽑는다. (비어 있으면 None)
	//==============================================================================
	pub fn pick<'a, T>(&mut self, array: &'a [T]) -> Option<&'a T> {
		if array.is_empty() {
			return None;
		}
		let index = self.nextInt(0, array.len() as i32 - 1);
		array.get(index as usize)
	}

	//==============================================================================
	// 배열을 제자리에서 섞는다. (Fisher-Yates, 원본과 같은 순서로 난수를 쓴다)
	//==============================================================================
	pub fn shuffle<T>(&mut self, array: &mut [T]) {
		let mut index = array.len();
		while index > 1 {
			index -= 1;
			let swapIndex = self.nextInt(0, index as i32) as usize;
			array.swap(index, swapIndex);
		}
	}

	//==============================================================================
	// 날짜 기반 시드를 만든다. (20260831 같은 YYYYMMDD 정수를 섞어, 같은 날짜면 어디서나 같은 시드가 나온다, 0 이면 1)
	//==============================================================================
	pub fn createDailySeed(dateNumber: u32, baseSeed: u32) -> u32 {
		let mixed = baseSeed ^ dateNumber.wrapping_mul(0x9E3779B1);
		normalizeSeed(mixed)
	}

	//==============================================================================
	// 날짜의 시드 난수를 만든다. (month 는 1 ~ 12, 원본 getMonth() + 1 자리)
	//==============================================================================
	pub fn fromDate(year: i32, month: u32, day: u32, baseSeed: u32) -> SeededRandom {
		let dateNumber = year as i64 * 10000 + month as i64 * 100 + day as i64;
		SeededRandom::new(SeededRandom::createDailySeed(dateNumber as u32, baseSeed))
	}
}

//==============================================================================
// 시드 0 을 1 로 바꾼다. (원본 || 1)
//==============================================================================
fn normalizeSeed(seed: u32) -> u32 {
	if seed == 0 {
		return 1;
	}
	seed
}

#[cfg(test)]
mod tests {
	use super::*;

	// 아래 기대값은 원본 D:\Github\vanilla.js\src\base\seededrandom.js 를 node v24.14.0 으로 그대로 불러 돌려 찍은 값이다.
	// (손 계산이 아니다. 같은 수열인지 f64 비트까지 같음 비교를 한다)

	//==============================================================================
	// 시드마다 첫 6 개 값과 상태가 원본과 같은지. (시드 0 은 1 과 같고, 2^32 - 1 과 0x9E3779B1 도 확인)
	//==============================================================================
	#[test]
	fn sequenceMatchesOriginal() {
		let cases: [(u32, [f64; 6], [u32; 6]); 5] = [
			(1, [0.6270739405881613, 0.002735721180215478, 0.5274470399599522, 0.9810509674716741, 0.9683778982143849, 0.281103502959013], [1831565814, 3663131627, 1199730144, 3031295957, 567894474, 2399460287]),
			(12345, [0.9797282677609473, 0.3067522644996643, 0.484205421525985, 0.817934412509203, 0.5094283693470061, 0.34747186047025025], [1831578158, 3663143971, 1199742488, 3031308301, 567906818, 2399472631]),
			(0, [0.6270739405881613, 0.002735721180215478, 0.5274470399599522, 0.9810509674716741, 0.9683778982143849, 0.281103502959013], [1831565814, 3663131627, 1199730144, 3031295957, 567894474, 2399460287]),
			(4294967295, [0.8964226141106337, 0.189478256739676, 0.7156526781618595, 0.9440599093213677, 0.8452364315744489, 0.5391399988438934], [1831565812, 3663131625, 1199730142, 3031295955, 567894472, 2399460285]),
			(0x9E3779B1, [0.5464560757391155, 0.45955629041418433, 0.2470416973810643, 0.5192999374121428, 0.8892884191591293, 0.702441792236641], [191034278, 2022600091, 3854165904, 1390764421, 3222330234, 758928751]),
		];
		for (seed, values, states) in cases.iter() {
			let mut random = SeededRandom::new(*seed);
			for (expectedValue, expectedState) in values.iter().zip(states.iter()) {
				let value = random.nextValue();
				assert_eq!(value.to_bits(), expectedValue.to_bits(), "seed {} value {} expected {}", seed, value, expectedValue);
				assert_eq!(random.getState(), *expectedState, "seed {}", seed);
			}
		}
	}

	//==============================================================================
	// nextInt, nextRange 가 원본과 같은지. (시드 777, nextInt(-5, 5) 10 번 뒤 nextRange(-2.5, 7.25) 5 번)
	//==============================================================================
	#[test]
	fn integerAndRangeMatchOriginal() {
		let mut random = SeededRandom::new(777);
		let integers: Vec<i32> = (0..10).map(|_| random.nextInt(-5, 5)).collect();
		assert_eq!(integers, vec![2, -5, -3, -4, 1, -1, -3, 3, 1, 3]);
		let expectedRanges = [4.358857887156773f64, 5.77185053244466, -1.1563375356490724, 4.196428792027291, 2.8847969832713716];
		for expectedRange in expectedRanges.iter() {
			assert_eq!(random.nextRange(-2.5, 7.25), *expectedRange as f32);
		}
		assert_eq!(random.getState(), 1703684196);
	}

	//==============================================================================
	// shuffle, pick 이 원본과 같은지. (시드 42, 0 ~ 9 섞은 뒤 a ~ d 에서 하나)
	//==============================================================================
	#[test]
	fn shuffleAndPickMatchOriginal() {
		let mut random = SeededRandom::new(42);
		let mut array = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
		random.shuffle(&mut array);
		assert_eq!(array, [0, 7, 3, 5, 2, 1, 8, 9, 4, 6]);
		assert_eq!(random.pick(&["a", "b", "c", "d"]), Some(&"b"));
		let empty: [i32; 0] = [];
		assert_eq!(random.pick(&empty), None);
	}

	//==============================================================================
	// 날짜 시드가 원본과 같은지. (createDailySeed(20261117), createDailySeed(20260831, 12345), fromDate(2026 년 11 월 17 일))
	//==============================================================================
	#[test]
	fn dailySeedMatchesOriginal() {
		assert_eq!(SeededRandom::createDailySeed(20261117, DAILY_BASE_SEED), 1951175260);
		assert_eq!(SeededRandom::createDailySeed(20260831, 12345), 679594006);
		assert_eq!(SeededRandom::fromDate(2026, 11, 17, DAILY_BASE_SEED).getState(), 1951175260);
	}

	//==============================================================================
	// 상태 저장과 복원, 시드 재설정.
	//==============================================================================
	#[test]
	fn saveAndRestoreState() {
		let mut random = SeededRandom::default();
		random.nextValue();
		let savedState = random.getState();
		let firstValues = [random.nextValue(), random.nextValue()];
		random.setState(savedState);
		assert_eq!([random.nextValue(), random.nextValue()], firstValues);
		random.setSeed(0);
		assert_eq!(random.getState(), 1);
		random.setState(0);
		assert_eq!(random.getState(), 0);
	}
}
