//==============================================================================
//! 솔버 안의 심볼. (원본 ui/autolayout/layoutsymbol.js 의 LayoutSymbol)
//! - tableau 의 행과 열을 가리킨다. 번호는 전역으로 하나씩 늘려 붙이고, 같음과 해시는 번호로 가린다.
//! - 원본과 다른 것:
//!   - 원본은 객체라 Map 의 키가 객체 동일성이다. 러스트 판은 번호와 종류를 담은 Copy 값이고 번호로 가린다.
//!   - 무효 심볼은 번호 0 하나를 같이 쓴다. (원본은 invalid() 마다 번호를 새로 붙인다. 무효 심볼은 키로 쓰이지 않아 결과는 같다)
//!   - 원본 setType 은 쓰는 곳이 없고 Copy 값에서는 뜻이 달라져서 옮기지 않았다.
//!   - LayoutSymbolMap 은 러스트 판에 더한 것이다. 원본이 행과 셀에 쓰는 Map 처럼 넣은 차례를 지킨다.
//!     (지웠다 다시 넣으면 맨 뒤로 가고, 있는 키의 값을 바꾸면 자리를 지킨다) 솔버가 셀과 행을 도는 차례가
//!     어느 심볼을 고를지 정하므로 원본과 같은 차례로 돈다.
//==============================================================================
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::ui::autolayout::layoutsymboltype::LayoutSymbolType;

/// 심볼마다 붙이는 번호의 다음 값. (0 은 무효 심볼)
static NEXT_SYMBOL_ID: AtomicU64 = AtomicU64::new(1);

//==============================================================================
/// 솔버 안의 심볼.
/// - 솔버 안에서 쓰는 값이다. 사용자 코드는 보통 LayoutVariable 과 LayoutSolver 만 쓴다.
///
/// # 예제
/// 번호로 가리는 심볼
/// ```rust
/// use vanilla::ui::autolayout::layoutsymbol::LayoutSymbol;
/// use vanilla::ui::autolayout::layoutsymboltype::LayoutSymbolType;
///
/// let first = LayoutSymbol::new(LayoutSymbolType::External);
/// let second = LayoutSymbol::new(LayoutSymbolType::External);
/// assert_ne!(first, second);
/// assert_eq!(first, first.clone());
/// assert!(LayoutSymbol::invalid().is_invalid());
/// assert_eq!(LayoutSymbol::invalid().get_id(), 0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct LayoutSymbol {
	id: u64,
	symbol_type: LayoutSymbolType,
}

impl LayoutSymbol {
	//==============================================================================
	/// 생성. (새 번호를 붙인다)
	///
	/// # 인자
	/// - `symbol_type`: 심볼 종류.
	///
	/// # 돌려줌
	/// 새 번호(1 부터 하나씩 늘어난다)를 붙인 심볼.
	//==============================================================================
	pub fn new(symbol_type: LayoutSymbolType) -> LayoutSymbol {
		let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::Relaxed);
		LayoutSymbol {
			id,
			symbol_type,
		}
	}

	//==============================================================================
	/// 무효 심볼.
	///
	/// # 돌려줌
	/// 번호 0, 종류 Invalid 인 심볼. (늘 같은 값)
	//==============================================================================
	pub fn invalid() -> LayoutSymbol {
		LayoutSymbol {
			id: 0,
			symbol_type: LayoutSymbolType::Invalid,
		}
	}

	//==============================================================================
	/// 번호 반환.
	///
	/// # 돌려줌
	/// 번호. (무효 심볼은 0)
	//==============================================================================
	pub fn get_id(&self) -> u64 {
		self.id
	}

	//==============================================================================
	/// 종류 반환.
	///
	/// # 돌려줌
	/// 종류.
	//==============================================================================
	pub fn get_type(&self) -> LayoutSymbolType {
		self.symbol_type
	}

	//==============================================================================
	/// 무효 심볼인지.
	///
	/// # 돌려줌
	/// 종류가 Invalid 면 true.
	//==============================================================================
	pub fn is_invalid(&self) -> bool {
		self.get_type() == LayoutSymbolType::Invalid
	}
}

impl PartialEq for LayoutSymbol {
	//==============================================================================
	/// 번호가 같은지.
	//==============================================================================
	fn eq(&self, other: &LayoutSymbol) -> bool {
		self.id == other.id
	}
}

impl Eq for LayoutSymbol {
}

impl Hash for LayoutSymbol {
	//==============================================================================
	/// 번호로 해시한다.
	//==============================================================================
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.id.hash(state);
	}
}

/// 지운 칸이 이 수를 넘고 살아 있는 칸보다 많으면 칸을 당겨 채운다.
const COMPACT_REMOVED_COUNT: usize = 16;

//==============================================================================
/// 넣은 차례를 지키는 심볼 키 맵. (원본 `Map<LayoutSymbol, V>` 자리)
/// - 칸 목록(Vec)에 넣은 차례대로 두고, 번호에서 칸 번호를 찾는 해시 맵으로 바로 찾는다.
/// - 지우면 칸을 비워 두고, 빈 칸이 많아지면 차례를 지킨 채 당겨 채운다.
///
/// # 예제
/// 넣은 차례를 지키는 맵
/// ```rust
/// use vanilla::ui::autolayout::layoutsymbol::{LayoutSymbol, LayoutSymbolMap};
/// use vanilla::ui::autolayout::layoutsymboltype::LayoutSymbolType;
///
/// let first = LayoutSymbol::new(LayoutSymbolType::Slack);
/// let second = LayoutSymbol::new(LayoutSymbolType::Error);
/// let mut map = LayoutSymbolMap::new();
/// map.insert(first, 1.0);
/// map.insert(second, 2.0);
/// map.insert(first, 10.0);
/// let values: Vec<f64> = map.iter().map(|(_, value)| *value).collect();
/// assert_eq!(values, vec![10.0, 2.0]);
///
/// assert_eq!(map.remove(&first), Some(10.0));
/// map.insert(first, 3.0);
/// let values: Vec<f64> = map.iter().map(|(_, value)| *value).collect();
/// assert_eq!(values, vec![2.0, 3.0]);
/// assert_eq!(map.len(), 2);
/// assert_eq!(map.get(&LayoutSymbol::invalid()), None);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct LayoutSymbolMap<V> {
	entries: Vec<Option<(LayoutSymbol, V)>>,
	indices: HashMap<u64, usize>,
	removed_count: usize,
}

impl<V> LayoutSymbolMap<V> {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 빈 맵.
	//==============================================================================
	pub fn new() -> LayoutSymbolMap<V> {
		LayoutSymbolMap {
			entries: Vec::new(),
			indices: HashMap::new(),
			removed_count: 0,
		}
	}

	//==============================================================================
	/// 들어 있는 수.
	///
	/// # 돌려줌
	/// 들어 있는 키 수. (지운 칸은 세지 않는다)
	//==============================================================================
	pub fn len(&self) -> usize {
		self.indices.len()
	}

	//==============================================================================
	/// 비었는지.
	///
	/// # 돌려줌
	/// 키가 하나도 없으면 true.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.indices.is_empty()
	}

	//==============================================================================
	/// 키가 있는지.
	///
	/// # 인자
	/// - `symbol`: 찾을 키.
	///
	/// # 돌려줌
	/// 키가 있으면 true.
	//==============================================================================
	pub fn contains_key(&self, symbol: &LayoutSymbol) -> bool {
		self.indices.contains_key(&symbol.id)
	}

	//==============================================================================
	/// 값 찾기.
	///
	/// # 인자
	/// - `symbol`: 찾을 키.
	///
	/// # 돌려줌
	/// 값의 참조. 키가 없으면 None.
	//==============================================================================
	pub fn get(&self, symbol: &LayoutSymbol) -> Option<&V> {
		let index = *self.indices.get(&symbol.id)?;
		match &self.entries[index] {
			Some((_, value)) => {
				Some(value)
			}
			None => {
				None
			}
		}
	}

	//==============================================================================
	/// 값 찾기. (바꿀 수 있게)
	///
	/// # 인자
	/// - `symbol`: 찾을 키.
	///
	/// # 돌려줌
	/// 바꿀 수 있는 값의 참조. 키가 없으면 None.
	//==============================================================================
	pub fn get_mut(&mut self, symbol: &LayoutSymbol) -> Option<&mut V> {
		let index = *self.indices.get(&symbol.id)?;
		match &mut self.entries[index] {
			Some((_, value)) => {
				Some(value)
			}
			None => {
				None
			}
		}
	}

	//==============================================================================
	/// 넣기. (있는 키는 자리를 지킨 채 값만 바꾸고, 새 키는 맨 뒤에 붙인다)
	///
	/// # 인자
	/// - `symbol`: 키.
	/// - `value`: 넣을 값.
	//==============================================================================
	pub fn insert(&mut self, symbol: LayoutSymbol, value: V) {
		if let Some(&index) = self.indices.get(&symbol.id) {
			self.entries[index] = Some((symbol, value));
			return;
		}
		self.indices.insert(symbol.id, self.entries.len());
		self.entries.push(Some((symbol, value)));
	}

	//==============================================================================
	/// 지우기. (지운 값을 돌려준다)
	///
	/// # 인자
	/// - `symbol`: 지울 키.
	///
	/// # 돌려줌
	/// 지운 값. 키가 없으면 None.
	//==============================================================================
	pub fn remove(&mut self, symbol: &LayoutSymbol) -> Option<V> {
		let index = self.indices.remove(&symbol.id)?;
		let entry = self.entries[index].take();
		self.removed_count += 1;
		if self.removed_count > COMPACT_REMOVED_COUNT && self.removed_count > self.indices.len() {
			self.compact();
		}
		match entry {
			Some((_, value)) => {
				Some(value)
			}
			None => {
				None
			}
		}
	}

	//==============================================================================
	/// 모두 지우기.
	//==============================================================================
	pub fn clear(&mut self) {
		self.entries.clear();
		self.indices.clear();
		self.removed_count = 0;
	}

	//==============================================================================
	/// 넣은 차례로 돌기.
	///
	/// # 돌려줌
	/// (키, 값의 참조) 를 넣은 차례로 내는 반복자.
	//==============================================================================
	pub fn iter(&self) -> impl Iterator<Item = (LayoutSymbol, &V)> {
		self.entries.iter().filter_map(|entry| {
			match entry {
				Some((symbol, value)) => {
					Some((*symbol, value))
				}
				None => {
					None
				}
			}
		})
	}

	//==============================================================================
	/// 넣은 차례로 돌기. (값을 바꿀 수 있게)
	///
	/// # 돌려줌
	/// (키, 바꿀 수 있는 값의 참조) 를 넣은 차례로 내는 반복자.
	//==============================================================================
	pub fn iter_mut(&mut self) -> impl Iterator<Item = (LayoutSymbol, &mut V)> {
		self.entries.iter_mut().filter_map(|entry| {
			match entry {
				Some((symbol, value)) => {
					Some((*symbol, value))
				}
				None => {
					None
				}
			}
		})
	}

	//==============================================================================
	/// 빈 칸을 차례를 지킨 채 당겨 채운다.
	//==============================================================================
	fn compact(&mut self) {
		let old_entries = std::mem::take(&mut self.entries);
		self.indices.clear();
		for entry in old_entries.into_iter().flatten() {
			self.indices.insert(entry.0.id, self.entries.len());
			self.entries.push(Some(entry));
		}
		self.removed_count = 0;
	}
}

impl<V> Default for LayoutSymbolMap<V> {
	//==============================================================================
	/// 빈 맵 생성. (LayoutSymbolMap::new 와 같다)
	//==============================================================================
	fn default() -> LayoutSymbolMap<V> {
		LayoutSymbolMap::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 원본 Map 처럼 넣은 차례를 지킨다. 지웠다 다시 넣으면 맨 뒤, 있는 키를 바꾸면 제자리. 당겨 채워도 차례는 같다.
	//==============================================================================
	#[test]
	fn symbol_map_keeps_insertion_order() {
		let symbols: Vec<LayoutSymbol> = (0..40).map(|_| LayoutSymbol::new(LayoutSymbolType::Slack)).collect();
		let mut map = LayoutSymbolMap::new();
		map.insert(symbols[0], 0.0);
		map.insert(symbols[1], 1.0);
		map.insert(symbols[2], 2.0);
		map.remove(&symbols[0]);
		map.insert(symbols[0], 10.0);
		map.insert(symbols[1], 11.0);
		let order: Vec<(u64, f64)> = map.iter().map(|(symbol, value)| (symbol.get_id(), *value)).collect();
		assert_eq!(order, vec![(symbols[1].get_id(), 11.0), (symbols[2].get_id(), 2.0), (symbols[0].get_id(), 10.0)]);
		for symbol in &symbols[3..] {
			map.insert(*symbol, symbol.get_id() as f64);
		}
		for symbol in &symbols[3..30] {
			map.remove(symbol);
		}
		let remaining: Vec<u64> = map.iter().map(|(symbol, _)| symbol.get_id()).collect();
		let mut expected = vec![symbols[1].get_id(), symbols[2].get_id(), symbols[0].get_id()];
		for symbol in &symbols[30..] {
			expected.push(symbol.get_id());
		}
		assert_eq!(remaining, expected);
		assert_eq!(map.len(), expected.len());
		assert_eq!(map.get(&symbols[35]), Some(&(symbols[35].get_id() as f64)));
		assert!(!map.contains_key(&symbols[10]));
		assert!(!map.contains_key(&LayoutSymbol::invalid()));
	}
}
