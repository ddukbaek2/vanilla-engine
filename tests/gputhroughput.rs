//==============================================================================
//! GPU 처리량 재기 시험. (원본 tests/core/gputhroughputchrome.test.mjs 와 같은 검사)
//! - Engine::run 이 도는 숨긴 창(찍기 모드 환경 변수)에서 씬의 initialize 가 Engine::measure_gpu_throughput 을 부른다.
//!   잰 프레임 수와 덩이 수, 프레임당 시간이 0 보다 큰지, 재는 동안 엔진 시간과 프레임 번호가 서 있는지, 씬의 tick 을 부르지 않는지,
//!   0 프레임과 재는 중 또 부르기는 Err 로 풀리는지, 다 재면 엔진 루프가 이어 도는지(tick 이 다시 불림) 본다.
//! - GL 컨텍스트를 짓지 못하면 실패한다. (말없이 건너뛰지 않는다)
//! - 실행: cargo test --test gputhroughput
//==============================================================================
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use vanilla::{Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};

struct ThroughputScene {
	scene_base: SceneBase,
	tick_count: Rc<Cell<u32>>,
	draw_count: Rc<Cell<u32>>,
	finished_tick_count: Rc<Cell<Option<u32>>>,
	/// 그릴 때마다 (엔진 시간, 프레임 번호).
	draw_log: Rc<RefCell<Vec<(f64, u64)>>>,
}

impl Scene for ThroughputScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	fn initialize(&mut self, engine: &Engine) {
		let tick_count = self.tick_count.clone();
		let draw_count = self.draw_count.clone();
		let finished_tick_count = self.finished_tick_count.clone();
		let draw_log = self.draw_log.clone();
		engine.spawn_task(|mut engine_context| async move {
			let zero_result = engine_context.with(|engine| engine.measure_gpu_throughput(0)).await;
			assert!(zero_result.is_err(), "0 프레임은 Err");
			let frame_before = engine_context.with(|engine| engine.get_frame_number());
			let time_before = engine_context.with(|engine| engine.get_time_manager().get_time());
			let tick_before = tick_count.get();
			let draw_before = draw_count.get();
			let first = engine_context.with(|engine| engine.measure_gpu_throughput(60));
			let second_result = engine_context.with(|engine| engine.measure_gpu_throughput(10)).await;
			assert!(second_result.is_err(), "재는 중 또 부르면 Err");
			let throughput = first.await.expect("잰 값");
			let frame_after = engine_context.with(|engine| engine.get_frame_number());
			let time_after = engine_context.with(|engine| engine.get_time_manager().get_time());
			assert_eq!(throughput.frame_count, 60, "잰 프레임 수");
			assert_eq!(throughput.chunk_frame_count, 30, "덩이 프레임 수");
			assert_eq!(throughput.chunk_milliseconds_per_frame.len(), 2, "덩이 수");
			assert!(throughput.milliseconds_per_frame > 0.0, "프레임당 시간이 0 보다 큼");
			// 부른 갱신은 그 프레임의 씬 그리기를 마저 하므로 하나가 더 든다.
			assert_eq!(draw_count.get() - draw_before, 1 + 10 + 60, "부른 프레임과 데움 10, 잰 60 프레임을 그림 ({})", draw_count.get() - draw_before);
			// 부른 프레임의 tick 은 이미 남았다. 그 뒤 재는 동안은 부르지 않는다.
			assert_eq!(tick_count.get(), tick_before + 1, "재는 동안 씬의 tick 을 부르지 않음");
			// 부른 프레임이 끝나며 하나 오르고, 다 재고 이어 도는 첫 프레임까지 한 프레임 간격 뒤다. 재는 동안은 서 있다.
			assert_eq!(frame_after, frame_before + 1, "프레임 번호가 서 있음");
			assert!(time_after >= time_before && time_after - time_before < 0.05, "엔진 시간이 한 프레임 안으로만 감 ({} -> {})", time_before, time_after);
			let draw_log = draw_log.borrow();
			let measured_log = &draw_log[draw_before as usize + 1..draw_before as usize + 1 + 70];
			assert!(measured_log.iter().all(|entry| *entry == measured_log[0]), "재는 동안 그린 70 프레임의 엔진 시간과 프레임 번호가 모두 같음");
			assert_eq!(measured_log[0], (draw_log[draw_before as usize].0, frame_before + 1), "재는 동안 시간은 부른 프레임 그대로, 번호는 부른 프레임이 끝난 값");
			println!("GPU 처리량 재기 시험: {:.3} 밀리초 / 프레임 (덩이 {:?})", throughput.milliseconds_per_frame, throughput.chunk_milliseconds_per_frame);
			// 엔진 루프가 이어 도는지는 씬의 tick 이 본다. (끝난 뒤 tick 이 다시 불리면 앞의 값이 끝난 때의 값)
			finished_tick_count.set(Some(tick_count.get()));
		});
	}

	fn tick(&mut self, _time_delta: f64) {
		self.tick_count.set(self.tick_count.get() + 1);
		if let Some(finished_tick_count) = self.finished_tick_count.get() {
			if self.tick_count.get() > finished_tick_count + 2 {
				Engine::get_engine().unwrap().exit_application();
			}
		}
	}

	fn draw(&mut self, graphic: &mut Graphic) {
		self.draw_count.set(self.draw_count.get() + 1);
		let engine = Engine::get_engine().unwrap();
		self.draw_log.borrow_mut().push((engine.get_time_manager().get_time(), engine.get_frame_number()));
		graphic.set_fill_color(vanilla::Color::create(0.1, 0.2, 0.3, 1.0));
		graphic.draw_rect(Rect::create(0.0, 0.0, 320.0, 180.0));
	}
}

fn main() {
	// 숨긴 창으로 돌린다. (찍기 모드, 저장할 프레임은 닿지 않는 번호)
	std::env::set_var("VANILLA_CAPTURE_PATH", std::env::temp_dir().join("gputhroughput_{frame}.png"));
	std::env::set_var("VANILLA_CAPTURE_FRAMES", "100000000");
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.reference_resolution_size = Vector2::create(320.0, 180.0);
	let engine = Engine::new(engine_configuration);
	let tick_count = Rc::new(Cell::new(0));
	let draw_count = Rc::new(Cell::new(0));
	let finished_tick_count = Rc::new(Cell::new(None));
	engine.run(Box::new(ThroughputScene { scene_base: SceneBase::new(), tick_count: tick_count.clone(), draw_count, finished_tick_count: finished_tick_count.clone(), draw_log: Rc::new(RefCell::new(Vec::new())) })).expect("엔진 실행");
	let finished = finished_tick_count.get().expect("재기가 끝나지 않았다");
	assert!(tick_count.get() > finished, "다 재면 엔진 루프가 이어 돎 (tick {} 번째에서 끝, 뒤 {} 번)", finished, tick_count.get());
	println!("GPU 처리량 재기 시험 통과");
}
