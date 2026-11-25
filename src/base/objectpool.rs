//==============================================================================
//! 오브젝트 풀. (원본 base/objectpool.js)
//! - 매 프레임 생성 / 해제되는 객체(탄환, 이펙트 등)를 재사용해 할당을 줄인다. (원본은 GC 압박을 줄이려고 만들었다)
//! - acquire() 로 빌리고 release() 로 되돌린다. 되돌릴 때 reset_handler 가 상태를 비운다.
//! - 러스트 판은 담는 타입을 T 로 고정한 제네릭이다. acquire 는 객체를 꺼내 넘겨주고(소유권 이동), release 는 다시 받는다.
//!   원본 release(null) 무시는 러스트에 null 이 없어서 뺐다.
//! - clear() 는 원본처럼 놀고 있는 목록만 비우고, 지금까지 만든 수(total_count)는 그대로 둔다.
//==============================================================================

/// 새 객체를 만드는 함수.
pub type ObjectCreateHandler<T> = Box<dyn FnMut() -> T>;
/// 되돌릴 때 상태를 비우는 함수. (객체를 인자로 받는다)
pub type ObjectResetHandler<T> = Box<dyn FnMut(&mut T)>;

//==============================================================================
/// 오브젝트 풀. (T 객체를 빌려주고 되돌려 받아 다시 쓴다)
///
/// # 예제
/// 탄환을 미리 만들어 두고 빌렸다 되돌리기
/// ```rust
/// use vanilla::ObjectPool;
///
/// struct Bullet {
/// 	travel_distance: f32,
/// }
///
/// let mut object_pool = ObjectPool::new(
/// 	Box::new(|| Bullet { travel_distance: 0.0 }),
/// 	Some(Box::new(|bullet: &mut Bullet| {
/// 		bullet.travel_distance = 0.0;
/// 	})),
/// 	2,
/// );
/// assert_eq!(object_pool.get_free_count(), 2);
/// let mut bullet = object_pool.acquire();
/// bullet.travel_distance = 120.0;
/// object_pool.release(bullet);
/// let reused_bullet = object_pool.acquire();
/// assert_eq!(reused_bullet.travel_distance, 0.0);
/// assert_eq!(object_pool.get_total_count(), 2);
/// ```
//==============================================================================
pub struct ObjectPool<T> {
	create_handler: ObjectCreateHandler<T>,
	reset_handler: Option<ObjectResetHandler<T>>,
	free_list: Vec<T>,
	total_count: usize,
}

impl<T> ObjectPool<T> {
	//==============================================================================
	/// 생성. (create_handler 는 새 객체를 만들고, reset_handler 는 되돌릴 때 상태를 비우고, preload_count 만큼 미리 만든다)
	///
	/// # 인자
	/// - `create_handler`: 새 객체를 만드는 함수. (미리 만들 때와 빌릴 때 놀고 있는 것이 없으면 부른다)
	/// - `reset_handler`: 되돌릴 때 상태를 비우는 함수. None 이면 비우지 않고 그대로 넣는다.
	/// - `preload_count`: 만들 때 미리 만들어 둘 객체 수.
	///
	/// # 돌려줌
	/// 놀고 있는 객체 preload_count 개를 든 풀. (전체 수도 preload_count)
	//==============================================================================
	pub fn new(create_handler: ObjectCreateHandler<T>, reset_handler: Option<ObjectResetHandler<T>>, preload_count: usize) -> ObjectPool<T> {
		let mut object_pool = ObjectPool {
			create_handler,
			reset_handler,
			free_list: Vec::with_capacity(preload_count),
			total_count: 0,
		};
		for _ in 0..preload_count {
			let instance = (object_pool.create_handler)();
			object_pool.free_list.push(instance);
			object_pool.total_count += 1;
		}
		object_pool
	}

	//==============================================================================
	/// 빌린다. (놀고 있는 것이 없으면 새로 만든다, 가장 나중에 되돌린 것부터 나간다)
	///
	/// # 돌려줌
	/// 빌린 객체. (소유권이 넘어온다, 다 쓰면 release 로 되돌린다)
	//==============================================================================
	pub fn acquire(&mut self) -> T {
		if let Some(instance) = self.free_list.pop() {
			return instance;
		}
		self.total_count += 1;
		(self.create_handler)()
	}

	//==============================================================================
	/// 되돌린다. (reset_handler 로 상태를 비운 뒤 놀고 있는 목록에 넣는다)
	///
	/// # 인자
	/// - `instance`: 되돌릴 객체. (이 풀에서 빌린 것인지 보지 않는다, 전체 수는 바꾸지 않는다)
	//==============================================================================
	pub fn release(&mut self, mut instance: T) {
		if let Some(reset_handler) = self.reset_handler.as_mut() {
			reset_handler(&mut instance);
		}
		self.free_list.push(instance);
	}

	//==============================================================================
	/// 놀고 있는 객체 수 반환.
	///
	/// # 돌려줌
	/// 놀고 있는 목록의 객체 수.
	//==============================================================================
	pub fn get_free_count(&self) -> usize {
		self.free_list.len()
	}

	//==============================================================================
	/// 지금까지 만든 전체 객체 수 반환.
	///
	/// # 돌려줌
	/// create_handler 로 만든 객체 수. (clear 해도 줄지 않는다)
	//==============================================================================
	pub fn get_total_count(&self) -> usize {
		self.total_count
	}

	//==============================================================================
	/// 풀을 비운다. (놀고 있는 목록만 비운다)
	//==============================================================================
	pub fn clear(&mut self) {
		self.free_list.clear();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::cell::Cell;
	use std::rc::Rc;

	#[derive(Debug, PartialEq)]
	struct Bullet {
		serial_number: u32,
		travel_distance: f32,
	}

	//==============================================================================
	/// 미리 만들기, 새로 만들기, 되돌릴 때 비우기, 재사용, 비우기.
	//==============================================================================
	#[test]
	fn reuse() {
		let next_serial_number = Rc::new(Cell::new(0u32));
		let serial_in_create = next_serial_number.clone();
		let reset_count = Rc::new(Cell::new(0u32));
		let reset_count_in_reset = reset_count.clone();
		let mut object_pool = ObjectPool::new(
			Box::new(move || {
				let serial_number = serial_in_create.get();
				serial_in_create.set(serial_number + 1);
				Bullet { serial_number, travel_distance: 0.0 }
			}),
			Some(Box::new(move |bullet: &mut Bullet| {
				bullet.travel_distance = 0.0;
				reset_count_in_reset.set(reset_count_in_reset.get() + 1);
			})),
			2,
		);
		assert_eq!(object_pool.get_free_count(), 2);
		assert_eq!(object_pool.get_total_count(), 2);

		let mut first = object_pool.acquire();
		let second = object_pool.acquire();
		let third = object_pool.acquire();
		assert_eq!(object_pool.get_free_count(), 0);
		assert_eq!(object_pool.get_total_count(), 3);
		assert_eq!(third.serial_number, 2);

		first.travel_distance = 120.0;
		let first_serial_number = first.serial_number;
		object_pool.release(first);
		assert_eq!(reset_count.get(), 1);
		assert_eq!(object_pool.get_free_count(), 1);

		let reused = object_pool.acquire();
		assert_eq!(reused, Bullet { serial_number: first_serial_number, travel_distance: 0.0 });
		assert_eq!(object_pool.get_total_count(), 3);

		object_pool.release(reused);
		object_pool.release(second);
		object_pool.release(third);
		assert_eq!(object_pool.get_free_count(), 3);
		object_pool.clear();
		assert_eq!(object_pool.get_free_count(), 0);
		assert_eq!(object_pool.get_total_count(), 3);
		assert_eq!(object_pool.acquire().serial_number, 3);
	}

	//==============================================================================
	/// reset_handler 없이도 쓴다.
	//==============================================================================
	#[test]
	fn without_reset_handler() {
		let mut object_pool = ObjectPool::new(Box::new(|| vec![0u8; 4]), None, 0);
		let mut buffer = object_pool.acquire();
		buffer[0] = 7;
		object_pool.release(buffer);
		assert_eq!(object_pool.acquire(), vec![7u8, 0, 0, 0]);
		assert_eq!(object_pool.get_total_count(), 1);
	}
}
