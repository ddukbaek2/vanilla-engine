//==============================================================================
// DOM 화면 분할 패널 예제. (원본 파티클 에디터의 창 구성과 같은 꼴)
// - 위 도구 막대(끌 수 없는 고정 40), 아래 가로 패널: 왼쪽 고정 240(위 320 과 아래 flex 로 세로 분할, 최소 120) | 오른쪽 flex(엔진 캔버스).
// - 도구 막대 단추: 왼쪽 패널 숨기기 / 보이기, 끌어서 바꾼 크기 지우기.
// - Pane 은 웹(wasm32)에만 있다. 데스크탑에서는 그렇다고 알리고 끝난다.
// - 실행(웹): cargo build --example panelayout --target wasm32-unknown-unknown 뒤 README 의 "웹으로 빌드" 처럼 wasm-bindgen 으로 묶어 띄운다.
//==============================================================================

#[cfg(target_arch = "wasm32")]
use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Pane, PaneDirection, PaneOptions, PaneSize, PaneStyle, PaneStyleOptions, PaneTheme, Rect, Scene, SceneBase, Vector2};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::closure::Closure;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast;

//==============================================================================
// 캔버스에 그리는 씬. (오른쪽 패널 안에서 보인다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
struct PreviewScene {
	scene_base: SceneBase,
}

#[cfg(target_arch = "wasm32")]
impl Scene for PreviewScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 이전 출력. (배경을 칠하고 기준 해상도 좌표계를 건다)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.scene_base.draw_scene_background(engine, graphic);
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		graphic.set_fill_color(Colors::VANILLA);
		graphic.draw_rect(Rect::create(60.0, 60.0, 240.0, 140.0));
	}
}

//==============================================================================
// 패널 위에 프리셋 패널 요소 하나와 제목 글을 붙인다.
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn add_panel_content(pane: &Pane, title: &str) -> web_sys::HtmlElement {
	let panel_element = PaneStyle::create("div", "panel", &PaneStyleOptions::default());
	let title_element = PaneStyle::create("div", "text", &PaneStyleOptions {
		text: title,
		style: &[("color", PaneTheme::COLOR_TEXT_DIM), ("font-size", PaneTheme::FONT_SIZE_SMALL), ("margin-bottom", "8px")],
		..Default::default()
	});
	let _ = panel_element.append_child(&title_element);
	let _ = pane.get_container().append_child(&panel_element);
	panel_element
}

//==============================================================================
// 단추에 누름 리스너를 단다.
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn add_click_listener(element: &web_sys::HtmlElement, mut click_handler: impl FnMut() + 'static) {
	let click_callback: Closure<dyn FnMut()> = Closure::new(move || {
		click_handler();
	});
	let _ = element.add_event_listener_with_callback("click", click_callback.as_ref().unchecked_ref());
	click_callback.forget();
}

//==============================================================================
// 창 구성. (뿌리 패널을 body 에 단다)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn build_layout() {
	let document = web_sys::window().and_then(|window| window.document()).expect("document 없음");
	let body = document.body().expect("body 없음");

	let root_pane = Pane::new(PaneOptions {
		direction: PaneDirection::Vertical,
		..Default::default()
	});

	// 위: 도구 막대.
	let toolbar_pane = Pane::new(PaneOptions {
		size: PaneSize::Fixed(40.0),
		min_size: 40.0,
		max_size: 40.0,
		is_resizable: false,
		..Default::default()
	});
	let toolbar_element = PaneStyle::create("div", "toolbar", &PaneStyleOptions::default());
	let title_element = PaneStyle::create("span", "text", &PaneStyleOptions {
		text: "PANE LAYOUT",
		..Default::default()
	});
	let toggle_button_element = PaneStyle::create("button", "button", &PaneStyleOptions {
		id: "toggleLeftButton",
		text: "Toggle Left",
		accent: true,
		..Default::default()
	});
	let reset_button_element = PaneStyle::create("button", "button", &PaneStyleOptions {
		id: "resetLayoutButton",
		text: "Reset Layout",
		warning: true,
		..Default::default()
	});
	let _ = toolbar_element.append_child(&title_element);
	let _ = toolbar_element.append_child(&toggle_button_element);
	let _ = toolbar_element.append_child(&reset_button_element);
	let _ = toolbar_pane.get_container().append_child(&toolbar_element);

	// 아래: 가로 패널.
	let main_pane = Pane::new(PaneOptions {
		direction: PaneDirection::Horizontal,
		..Default::default()
	});

	// 왼쪽: 위아래 둘로 나눈 고정 폭 패널.
	let left_pane = Pane::new(PaneOptions {
		direction: PaneDirection::Vertical,
		size: PaneSize::Fixed(240.0),
		min_size: 170.0,
		..Default::default()
	});
	let top_pane = Pane::new(PaneOptions {
		size: PaneSize::Fixed(320.0),
		min_size: 120.0,
		..Default::default()
	});
	add_panel_content(&top_pane, "PRESETS");
	let bottom_pane = Pane::new(PaneOptions {
		min_size: 120.0,
		..Default::default()
	});
	let bottom_panel_element = add_panel_content(&bottom_pane, "HIERARCHY");
	let search_element = PaneStyle::create("input", "input", &PaneStyleOptions {
		element_type: "text",
		placeholder: "Search",
		..Default::default()
	});
	let _ = bottom_panel_element.append_child(&search_element);
	left_pane.add_pane(&top_pane);
	left_pane.add_pane(&bottom_pane);

	// 오른쪽: 엔진 캔버스를 담는 flex 패널. (엔진은 canvas_id 로 이 캔버스를 찾는다)
	let preview_pane = Pane::new(PaneOptions {
		min_size: 200.0,
		..Default::default()
	});
	let canvas_element = PaneStyle::create("canvas", "", &PaneStyleOptions {
		id: "mainCanvas",
		..Default::default()
	});
	let _ = preview_pane.get_container().append_child(&canvas_element);

	main_pane.add_pane(&left_pane);
	main_pane.add_pane(&preview_pane);
	root_pane.add_pane(&toolbar_pane);
	root_pane.add_pane(&main_pane);
	root_pane.attach_to(&body);

	let toggle_left_pane = left_pane.clone();
	add_click_listener(&toggle_button_element, move || {
		toggle_left_pane.set_visible(!toggle_left_pane.is_visible());
	});
	let reset_root_pane = root_pane.clone();
	add_click_listener(&reset_button_element, move || {
		reset_root_pane.reset_layout();
		reset_root_pane.refresh();
	});
}

//==============================================================================
// 시작. (웹)
//==============================================================================
#[cfg(target_arch = "wasm32")]
fn main() {
	build_layout();
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("바닐라엔진 패널 배치");
	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
	let engine = Engine::new(engine_configuration);
	let preview_scene = PreviewScene {
		scene_base: SceneBase::new(),
	};
	if let Err(error) = engine.run(Box::new(preview_scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}

//==============================================================================
// 시작. (데스크탑에는 DOM 이 없다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn main() {
	println!("Pane 은 웹(wasm32) 빌드에만 있습니다. cargo build --example panelayout --target wasm32-unknown-unknown 으로 빌드해 브라우저에서 띄우세요.");
}
