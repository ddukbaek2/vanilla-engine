//==============================================================================
//! 전역 엔진. (원본 Engine.getEngine() 의 System.vanillaEngine 자리. 언리얼 GEngine, 고도 Engine 싱글톤과 같은 꼴)
//! - 엔진은 처음 한 번 만들고 지우지 않는다. Engine::new 가 엔진을 프로그램 끝까지 사는 자리('static)에 두고 &'static Engine 을 준다.
//!   그 자리를 스레드 칸에 적어 두어, 만든 직후부터 씬 함수, draw, 컴포넌트, 위젯 콜백, async 작업 안 어디서나 Engine::get_engine() 이 같은 엔진을 준다.
//!   원본처럼 마지막으로 만든 엔진을 준다.
//! - 엔진의 공개 함수는 모두 &self 다. 고치는 칸은 안쪽 가변성(Cell, RefCell)으로 둔다. (고도의 러스트 연결 godot-rust 싱글톤과 Gd 의 bind, bind_mut 꼴)
//!   그래서 씬 함수가 받은 engine 인자와 Engine::get_engine() 이 같은 엔진을 동시에 써도 러스트 빌림 규칙이 깨지지 않는다.
//! - 매니저(시간, 뷰, 입력, 포인터, 소리, 그래픽)는 RefCell 안에 있다. get_x 는 Ref, get_x_mut 은 RefMut 을 준다.
//!   같은 매니저를 RefMut 으로 쥔 채 또 빌리면(get_x 나 get_x_mut) 함수 이름과 매니저 이름을 적은 패닉으로 멈춘다.
//!   빌린 것은 한 문장 안에서 쓰고 버린다. (let 으로 오래 쥐고 다른 엔진 함수를 부르지 않는다)
//! - 다른 스레드에서 부르면 Err(EngineAccessError::OtherThread) 다. (엔진은 만든 스레드 전용이다. Engine 은 Sync 가 아니라 &Engine 을 다른 스레드로 보낼 수 없다. 웹은 스레드가 하나)
//==============================================================================
use crate::core::engine::Engine;
use std::cell::{Cell, Ref, RefCell, RefMut};
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};

/// 어느 스레드에서든 엔진을 만든 적이 있는지. (다른 스레드에서 꺼낼 때 OtherThread 를 알리려고)
static IS_ANY_ENGINE_CREATED: AtomicBool = AtomicBool::new(false);

thread_local! {
	/// 이 스레드에서 마지막으로 만든 엔진.
	static GLOBAL_ENGINE: Cell<Option<&'static Engine>> = const { Cell::new(None) };
}

//==============================================================================
/// 전역 엔진을 꺼내지 못한 까닭. (Engine::get_engine)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineAccessError {
	/// 엔진을 만든 적이 없다. (원본 getEngine 이 undefined 인 자리)
	NotCreated,
	/// 엔진은 다른 스레드(엔진을 만든 주 스레드)에 있다. 엔진은 그 스레드에서만 꺼낸다.
	OtherThread,
}

impl fmt::Display for EngineAccessError {
	//==============================================================================
	/// 까닭 글.
	//==============================================================================
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		let message = match self {
			EngineAccessError::NotCreated => "Engine::get_engine: 엔진을 만든 적이 없습니다",
			EngineAccessError::OtherThread => "Engine::get_engine: 엔진은 엔진을 만든 스레드에서만 꺼낼 수 있습니다",
		};
		formatter.write_str(message)
	}
}

impl std::error::Error for EngineAccessError {}

//==============================================================================
/// 엔진을 프로그램 끝까지 사는 자리에 두고 전역 엔진으로 적기. (Engine::new)
/// - 엔진은 지우지 않는다. (원본 System.vanillaEngine 처럼 프로세스 끝까지 산다)
//==============================================================================
pub(crate) fn install_global_engine(engine: Engine) -> &'static Engine {
	let engine: &'static Engine = Box::leak(Box::new(engine));
	IS_ANY_ENGINE_CREATED.store(true, Ordering::Relaxed);
	GLOBAL_ENGINE.with(|engine_cell| engine_cell.set(Some(engine)));
	engine
}

//==============================================================================
/// 꺼내기. (Engine::get_engine)
//==============================================================================
pub(crate) fn get_global_engine() -> Result<&'static Engine, EngineAccessError> {
	let engine = GLOBAL_ENGINE.try_with(|engine_cell| engine_cell.get()).ok().flatten();
	match engine {
		Some(engine) => {
			Ok(engine)
		}
		None => {
			if IS_ANY_ENGINE_CREATED.load(Ordering::Relaxed) {
				Err(EngineAccessError::OtherThread)
			}
			else {
				Err(EngineAccessError::NotCreated)
			}
		}
	}
}

//==============================================================================
/// 엔진 칸 읽기 빌림. (겹쳐 빌리면 함수 이름과 칸 이름을 적고 패닉한다)
//==============================================================================
#[track_caller]
pub(crate) fn borrow_engine_field<'a, T: ?Sized>(field_cell: &'a RefCell<T>, function_name: &str, field_name: &str) -> Ref<'a, T> {
	match field_cell.try_borrow() {
		Ok(field) => {
			field
		}
		Err(_) => {
			panic!("{function_name}: 엔진의 {field_name} 칸을 이미 고치는 중이라 빌릴 수 없습니다 (get_x_mut 으로 받은 것을 쥔 채 같은 칸을 또 빌렸습니다. 빌린 것은 한 문장 안에서 쓰고 버립니다)");
		}
	}
}

//==============================================================================
/// 엔진 칸 고치기 빌림. (겹쳐 빌리면 함수 이름과 칸 이름을 적고 패닉한다)
//==============================================================================
#[track_caller]
pub(crate) fn borrow_engine_field_mut<'a, T: ?Sized>(field_cell: &'a RefCell<T>, function_name: &str, field_name: &str) -> RefMut<'a, T> {
	match field_cell.try_borrow_mut() {
		Ok(field) => {
			field
		}
		Err(_) => {
			panic!("{function_name}: 엔진의 {field_name} 칸을 이미 빌려 쓰는 중이라 고칠 수 없습니다 (get_x 나 get_x_mut 으로 받은 것을 쥔 채 같은 칸을 또 빌렸습니다. 빌린 것은 한 문장 안에서 쓰고 버립니다)");
		}
	}
}
