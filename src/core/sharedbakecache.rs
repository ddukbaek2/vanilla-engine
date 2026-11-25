//==============================================================================
// 구운 그림 나눠 쓰기. (vanilla.js core/graphic/sharedbakecache.js, ShadowText, ShadowImage, SoftShade 가 쓴다)
// - 같은 모양(열쇠)이면 한 번만 굽고 여러 컴포넌트가 같은 그림(Rc)을 그린다. GPU 텍스처도 그림마다 하나라 함께 준다.
// - 원본은 열쇠마다 쓰는 컴포넌트 수를 세고 놓기(release)를 부른다. 러스트 판은 컴포넌트가 Rc 를 쥐는 것이 곧 참조 수다.
//   열쇠가 바뀌어 다른 것을 쥐거나 컴포넌트가 사라지면 Rc 가 놓인다. 캐시만 쥔 것(Rc::strong_count 가 1)이 아무도 쓰지 않는 것이다.
// - 아무도 쓰지 않는 것은 바로 버리지 않고 남겨 둔다. (같은 모양이 곧 다시 오면 굽지 않는다) 새로 담을 때 MAXIMUM_IDLE_BAKE_COUNT 를 넘었으면 오래 안 쓴 것부터 버린다.
// - 굽지 못한 것(None)은 담지 않는다. 엔진은 한 스레드에서 돌아 캐시는 스레드 하나의 전역이다.
//==============================================================================
use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

// 아무도 쓰지 않는 구운 그림을 남겨 두는 수. (원본과 같음)
const MAXIMUM_IDLE_BAKE_COUNT: usize = 128;

struct SharedBakeEntry {
	bake: Rc<dyn Any>,
	last_used_count: u64,
}

struct SharedBakeState {
	entry_table: HashMap<String, SharedBakeEntry>,
	use_count: u64,
}

thread_local! {
	static SHARED_BAKE_STATE: RefCell<SharedBakeState> = RefCell::new(SharedBakeState {
		entry_table: HashMap::new(),
		use_count: 0,
	});
}

//==============================================================================
// 구운 그림 쥐기. (열쇠에 구운 것이 있으면 그것을, 없으면 create_bake 로 구워 담고 준다, 원본 acquireSharedBake)
// - 돌려준 Rc 를 쥐고 있는 동안은 버리지 않는다. 열쇠가 같으면 종류(T)도 같아야 한다. (종류가 다르면 새로 굽고 바꿔 담는다)
//==============================================================================
pub fn acquire_shared_bake<T: 'static>(bake_key: &str, create_bake: impl FnOnce() -> Option<Rc<T>>) -> Option<Rc<T>> {
	let cached_bake = SHARED_BAKE_STATE.with(|shared_bake_state| {
		let mut shared_bake_state = shared_bake_state.borrow_mut();
		shared_bake_state.use_count += 1;
		let use_count = shared_bake_state.use_count;
		let entry = shared_bake_state.entry_table.get_mut(bake_key)?;
		entry.last_used_count = use_count;
		entry.bake.clone().downcast::<T>().ok()
	});
	if cached_bake.is_some() {
		return cached_bake;
	}
	let bake = create_bake()?;
	SHARED_BAKE_STATE.with(|shared_bake_state| {
		let mut shared_bake_state = shared_bake_state.borrow_mut();
		let use_count = shared_bake_state.use_count;
		let shared_bake: Rc<dyn Any> = bake.clone();
		shared_bake_state.entry_table.insert(bake_key.to_string(), SharedBakeEntry {
			bake: shared_bake,
			last_used_count: use_count,
		});
		evict_idle_bakes(&mut shared_bake_state);
	});
	Some(bake)
}

//==============================================================================
// 아무도 쓰지 않는 것이 넘치면 오래 안 쓴 것부터 버리기.
//==============================================================================
fn evict_idle_bakes(shared_bake_state: &mut SharedBakeState) {
	let mut idle_list: Vec<(u64, String)> = shared_bake_state.entry_table.iter().filter(|(_, entry)| Rc::strong_count(&entry.bake) <= 1).map(|(bake_key, entry)| (entry.last_used_count, bake_key.clone())).collect();
	if idle_list.len() <= MAXIMUM_IDLE_BAKE_COUNT {
		return;
	}
	idle_list.sort();
	let evict_count = idle_list.len() - MAXIMUM_IDLE_BAKE_COUNT;
	for (_, bake_key) in idle_list.iter().take(evict_count) {
		shared_bake_state.entry_table.remove(bake_key);
	}
}

//==============================================================================
// 지금 들고 있는 구운 그림 수와 그 가운데 아무도 쓰지 않는 수. (시험과 통계용, 원본 readSharedBakeCount)
//==============================================================================
pub fn read_shared_bake_count() -> (usize, usize) {
	SHARED_BAKE_STATE.with(|shared_bake_state| {
		let shared_bake_state = shared_bake_state.borrow();
		let idle_count = shared_bake_state.entry_table.values().filter(|entry| Rc::strong_count(&entry.bake) <= 1).count();
		(shared_bake_state.entry_table.len(), idle_count)
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 같은 열쇠는 한 번만 굽고 같은 Rc 를 준다. 쥔 것이 없으면 아무도 쓰지 않는 것으로 센다.
	//==============================================================================
	#[test]
	fn same_key_bakes_once_and_counts_idle() {
		let mut bake_count = 0;
		let first_bake = acquire_shared_bake("test|a", || {
			bake_count += 1;
			Some(Rc::new(String::from("a")))
		}).expect("first");
		let second_bake = acquire_shared_bake("test|a", || {
			bake_count += 1;
			Some(Rc::new(String::from("a2")))
		}).expect("second");
		assert_eq!(bake_count, 1);
		assert!(Rc::ptr_eq(&first_bake, &second_bake));
		assert_eq!(read_shared_bake_count(), (1, 0));
		drop(first_bake);
		drop(second_bake);
		assert_eq!(read_shared_bake_count(), (1, 1));
		let none_bake: Option<Rc<String>> = acquire_shared_bake("test|none", || None);
		assert!(none_bake.is_none());
		assert_eq!(read_shared_bake_count(), (1, 1));
	}

	//==============================================================================
	// 아무도 쓰지 않는 것이 상한을 넘으면 오래 안 쓴 것부터 버리고, 쥔 것은 남긴다.
	//==============================================================================
	#[test]
	fn evicts_oldest_idle_only() {
		let held_bake = acquire_shared_bake("held", || Some(Rc::new(0usize))).expect("held");
		for bake_index in 0..(MAXIMUM_IDLE_BAKE_COUNT + 20) {
			let bake_key = format!("idle|{}", bake_index);
			acquire_shared_bake(&bake_key, || Some(Rc::new(bake_index)));
		}
		// 버리기는 담을 때 하므로, 마지막에 담은 것(그때는 돌려주려고 쥐고 있었다)만큼 하나 넘을 수 있다.
		let (bake_count, idle_count) = read_shared_bake_count();
		assert_eq!(idle_count, MAXIMUM_IDLE_BAKE_COUNT + 1);
		assert_eq!(bake_count, MAXIMUM_IDLE_BAKE_COUNT + 2);
		let mut is_rebaked = false;
		acquire_shared_bake("idle|0", || {
			is_rebaked = true;
			Some(Rc::new(0usize))
		});
		assert!(is_rebaked);
		assert_eq!(*held_bake, 0);
	}
}
