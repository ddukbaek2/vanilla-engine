//==============================================================================
// 게임패드 목록 예제.
// - GamepadManager 를 만들어 1초 동안 갱신하면서 연결된 게임패드를 출력하고 끝난다. (창 없이 돈다)
// - 윈도우의 gilrs(Windows.Gaming.Input)는 포커스된 창이 있어야 입력 이벤트를 받는다. 창이 없는 이 예제는 연결 목록만 본다.
// - 실행: cargo run --example gamepadlist
//==============================================================================
#![allow(non_snake_case)]

use std::time::{Duration, Instant};
use vanilla::GamepadManager;

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut gamepadManager = GamepadManager::new();
	let frameInterval = Duration::from_millis(16);
	let runDuration = Duration::from_secs(1);
	let startInstant = Instant::now();
	while startInstant.elapsed() < runDuration {
		let timeDelta = frameInterval.as_secs_f32();
		gamepadManager.tick(timeDelta);
		std::thread::sleep(frameInterval);
	}

	let connectedGamepadCount = gamepadManager.getConnectedGamepadCount();
	println!("연결된 게임패드: {}개", connectedGamepadCount);
	for gamepad in gamepadManager.getAllConnectedGamepads() {
		println!("- index {}: {} (버튼 {}개, 축 {}개)", gamepad.index, gamepad.id, gamepad.buttons.len(), gamepad.axes.len());
	}
}
