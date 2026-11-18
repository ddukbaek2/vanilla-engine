//==============================================================================
// 오브젝트 풀. (원본 base/objectpool.js)
// - 매 프레임 생성 / 해제되는 객체(탄환, 이펙트 등)를 재사용해 할당을 줄인다. (원본은 GC 압박을 줄이려고 만들었다)
// - acquire() 로 빌리고 release() 로 되돌린다. 되돌릴 때 resetHandler 가 상태를 비운다.
// - 러스트 판은 담는 타입을 T 로 고정한 제네릭이다. acquire 는 객체를 꺼내 넘겨주고(소유권 이동), release 는 다시 받는다.
//   원본 release(null) 무시는 러스트에 null 이 없어서 뺐다.
// - clear() 는 원본처럼 놀고 있는 목록만 비우고, 지금까지 만든 수(totalCount)는 그대로 둔다.
//==============================================================================

// 새 객체를 만드는 함수.
pub type ObjectCreateHandler<T> = Box<dyn FnMut() -> T>;
// 되돌릴 때 상태를 비우는 함수. (객체를 인자로 받는다)
pub type ObjectResetHandler<T> = Box<dyn FnMut(&mut T)>;

pub struct ObjectPool<T> {
	createHandler: ObjectCreateHandler<T>,
	resetHandler: Option<ObjectResetHandler<T>>,
	freeList: Vec<T>,
	totalCount: usize,
}

impl<T> ObjectPool<T> {
	//==============================================================================
	// 생성. (createHandler 는 새 객체를 만들고, resetHandler 는 되돌릴 때 상태를 비우고, preloadCount 만큼 미리 만든다)
	//==============================================================================
	pub fn new(createHandler: ObjectCreateHandler<T>, resetHandler: Option<ObjectResetHandler<T>>, preloadCount: usize) -> ObjectPool<T> {
		let mut objectPool = ObjectPool {
			createHandler,
			resetHandler,
			freeList: Vec::with_capacity(preloadCount),
			totalCount: 0,
		};
		for _ in 0..preloadCount {
			let instance = (objectPool.createHandler)();
			objectPool.freeList.push(instance);
			objectPool.totalCount += 1;
		}
		objectPool
	}

	//==============================================================================
	// 빌린다. (놀고 있는 것이 없으면 새로 만든다, 가장 나중에 되돌린 것부터 나간다)
	//==============================================================================
	pub fn acquire(&mut self) -> T {
		if let Some(instance) = self.freeList.pop() {
			return instance;
		}
		self.totalCount += 1;
		(self.createHandler)()
	}

	//==============================================================================
	// 되돌린다. (resetHandler 로 상태를 비운 뒤 놀고 있는 목록에 넣는다)
	//==============================================================================
	pub fn release(&mut self, mut instance: T) {
		if let Some(resetHandler) = self.resetHandler.as_mut() {
			resetHandler(&mut instance);
		}
		self.freeList.push(instance);
	}

	//==============================================================================
	// 놀고 있는 객체 수 반환.
	//==============================================================================
	pub fn getFreeCount(&self) -> usize {
		self.freeList.len()
	}

	//==============================================================================
	// 지금까지 만든 전체 객체 수 반환.
	//==============================================================================
	pub fn getTotalCount(&self) -> usize {
		self.totalCount
	}

	//==============================================================================
	// 풀을 비운다. (놀고 있는 목록만 비운다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.freeList.clear();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	#[derive(Debug, PartialEq)]
	struct Bullet {
		serialNumber: u32,
		travelDistance: f32,
	}

	//==============================================================================
	// 미리 만들기, 새로 만들기, 되돌릴 때 비우기, 재사용, 비우기.
	//==============================================================================
	#[test]
	fn reuse() {
		let nextSerialNumber = Rc::new(Cell::new(0u32));
		let serialInCreate = nextSerialNumber.clone();
		let resetCount = Rc::new(Cell::new(0u32));
		let resetCountInReset = resetCount.clone();
		let mut objectPool = ObjectPool::new(
			Box::new(move || {
				let serialNumber = serialInCreate.get();
				serialInCreate.set(serialNumber + 1);
				Bullet { serialNumber, travelDistance: 0.0 }
			}),
			Some(Box::new(move |bullet: &mut Bullet| {
				bullet.travelDistance = 0.0;
				resetCountInReset.set(resetCountInReset.get() + 1);
			})),
			2,
		);
		assert_eq!(objectPool.getFreeCount(), 2);
		assert_eq!(objectPool.getTotalCount(), 2);

		let mut first = objectPool.acquire();
		let second = objectPool.acquire();
		let third = objectPool.acquire();
		assert_eq!(objectPool.getFreeCount(), 0);
		assert_eq!(objectPool.getTotalCount(), 3);
		assert_eq!(third.serialNumber, 2);

		first.travelDistance = 120.0;
		let firstSerialNumber = first.serialNumber;
		objectPool.release(first);
		assert_eq!(resetCount.get(), 1);
		assert_eq!(objectPool.getFreeCount(), 1);

		let reused = objectPool.acquire();
		assert_eq!(reused, Bullet { serialNumber: firstSerialNumber, travelDistance: 0.0 });
		assert_eq!(objectPool.getTotalCount(), 3);

		objectPool.release(reused);
		objectPool.release(second);
		objectPool.release(third);
		assert_eq!(objectPool.getFreeCount(), 3);
		objectPool.clear();
		assert_eq!(objectPool.getFreeCount(), 0);
		assert_eq!(objectPool.getTotalCount(), 3);
		assert_eq!(objectPool.acquire().serialNumber, 3);
	}

	//==============================================================================
	// resetHandler 없이도 쓴다.
	//==============================================================================
	#[test]
	fn withoutResetHandler() {
		let mut objectPool = ObjectPool::new(Box::new(|| vec![0u8; 4]), None, 0);
		let mut buffer = objectPool.acquire();
		buffer[0] = 7;
		objectPool.release(buffer);
		assert_eq!(objectPool.acquire(), vec![7u8, 0, 0, 0]);
		assert_eq!(objectPool.getTotalCount(), 1);
	}
}
