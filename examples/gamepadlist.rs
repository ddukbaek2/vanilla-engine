//==============================================================================
// 게임패드 목록 예제.
// - GamepadManager 를 만들어 1초 동안 갱신하면서 연결된 게임패드를 출력하고 끝난다. (창 없이 돈다)
// - 윈도우의 gilrs(Windows.Gaming.Input)는 포커스된 창이 있어야 입력 이벤트를 받는다. 창이 없는 이 예제는 연결 목록만 본다.
// - 실행: cargo run --example gamepadlist
//==============================================================================

use std::time::{Duration, Instant};
use vanilla::GamepadManager;

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut gamepad_manager = GamepadManager::new();
	let frame_interval = Duration::from_millis(16);
	let run_duration = Duration::from_secs(1);
	let start_instant = Instant::now();
	while start_instant.elapsed() < run_duration {
		let time_delta = frame_interval.as_secs_f32();
		gamepad_manager.tick(time_delta);
		std::thread::sleep(frame_interval);
	}

	let connected_gamepad_count = gamepad_manager.get_connected_gamepad_count();
	println!("연결된 게임패드: {}개", connected_gamepad_count);
	for gamepad in gamepad_manager.get_all_connected_gamepads() {
		println!("- index {}: {} (버튼 {}개, 축 {}개)", gamepad.index, gamepad.id, gamepad.buttons.len(), gamepad.axes.len());
	}
}
