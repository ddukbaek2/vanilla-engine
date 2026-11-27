//==============================================================================
// 글리프 래스터 견주기 예제. (원본 tests/assets/glyphparity/index.html 과 같은 장면)
// - 10 ~ 48px 글, serif, 굵게, 기울임, 한글, 이모지, 소수 자리, 빨간 글을 흰 바탕(위 절반)과 어두운 바탕(아래 절반)에 그린다.
// - CI 가 같은 기계의 크롬으로 원본 페이지를 찍어 글자 구역의 화소 차이를 센다. (.github/scripts/glyphcompare.cjs)
// - 실행: cargo run --example glyphparity
//==============================================================================

use vanilla::{Color, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, TextAlign, TextBaseline, Vector2};

/// 논리 화면 너비.
const VIEW_WIDTH: f64 = 960.0;
/// 논리 화면 높이.
const VIEW_HEIGHT: f64 = 720.0;
/// 크기 줄의 글자 크기. (픽셀)
const SIZE_LIST: [f64; 10] = [10.0, 12.0, 13.0, 14.0, 16.0, 18.0, 20.0, 24.0, 32.0, 48.0];
/// 크기 줄의 글.
const SAMPLE_TEXT: &str = "Hamburgefonstiv 0123 Ag";
/// 오른쪽 줄. (폰트 문자열, 글, 가로 자리)
const LINE_LIST: [(&str, &str, f64); 9] = [
	("16px serif", "Serif Hamburgefonstiv 0123", 545.0),
	("bold 16px sans-serif", "Bold Hamburgefonstiv 0123", 545.0),
	("italic 16px sans-serif", "Italic Hamburgefonstiv", 545.0),
	("16px sans-serif", "다람쥐 헌 쳇바퀴에 타고파", 545.0),
	("24px sans-serif", "다람쥐 헌 쳇바퀴", 545.0),
	("24px sans-serif", "\u{1F600}\u{1F3AE}\u{1F44D}\u{2764}\u{FE0F}", 545.0),
	("16px sans-serif", "Subpixel 0.37 offset", 545.37),
	("15px sans-serif", "Subpixel 0.62 offset", 545.62),
	("13px sans-serif", "The quick brown fox jumps", 545.0),
];

//==============================================================================
// 반쪽 그리기. (위 끝 y 와 글 색)
//==============================================================================
fn draw_half(graphic: &mut Graphic, top_y: f64, text_color: Color) {
	graphic.set_fill_color(text_color);
	let mut baseline_y = top_y + 4.0;
	for size in SIZE_LIST {
		baseline_y += size * 1.2 + 2.0;
		graphic.set_font_string(&format!("{}px sans-serif", size));
		graphic.draw_fill_text(SAMPLE_TEXT, 10.0, baseline_y);
	}
	let mut line_y = top_y + 24.0;
	for (font_string, text, position_x) in LINE_LIST {
		graphic.set_font_string(font_string);
		graphic.draw_fill_text(text, position_x, line_y);
		line_y += 34.0;
	}
	graphic.set_fill_color(Color::create_from_bytes(220, 40, 40, 255));
	graphic.set_font_string("18px sans-serif");
	graphic.draw_fill_text("Red Hamburgefonstiv", 545.0, line_y);
}

//==============================================================================
// 첫 씬.
//==============================================================================
struct MainScene {
	scene_base: SceneBase,
}

impl Scene for MainScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 생성. (원본처럼 렌더 배율 상한 1)
	//==============================================================================
	fn load(&mut self, engine: &Engine) {
		engine.get_view_manager_mut().set_max_render_pixel_ratio(1.0);
	}

	//==============================================================================
	// 그리기.
	//==============================================================================
	fn draw(&mut self, graphic: &mut Graphic) {
		graphic.set_text_baseline(TextBaseline::Alphabetic);
		graphic.set_text_align(TextAlign::Left);
		graphic.set_fill_color(Color::create_from_bytes(255, 255, 255, 255));
		graphic.draw_rect(Rect::create(0.0, 0.0, VIEW_WIDTH, VIEW_HEIGHT / 2.0));
		graphic.set_fill_color(Color::create_from_bytes(32, 32, 32, 255));
		graphic.draw_rect(Rect::create(0.0, VIEW_HEIGHT / 2.0, VIEW_WIDTH, VIEW_HEIGHT / 2.0));
		draw_half(graphic, 0.0, Color::create_from_bytes(0, 0, 0, 255));
		draw_half(graphic, VIEW_HEIGHT / 2.0, Color::create_from_bytes(255, 255, 255, 255));
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("글리프 래스터 견주기 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	let engine = Engine::new(engine_configuration);
	let main_scene = MainScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(main_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
