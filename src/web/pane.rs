//==============================================================================
// DOM 화면 분할 패널. (PaneTheme, PaneStyle, Pane)
// - 원본 web/pane.js 를 옮긴 것이다. 웹(wasm32) 빌드에만 있다. 데스크탑에는 DOM 이 없어서 옮길 자리가 없다.
//     let main_pane = Pane::new(PaneOptions { direction: PaneDirection::Horizontal, ..Default::default() });
//     let left_pane = Pane::new(PaneOptions { size: PaneSize::Fixed(240.0), min_size: 170.0, ..Default::default() });
//     let view_pane = Pane::new(PaneOptions { min_size: 200.0, ..Default::default() });
//     main_pane.add_pane(&left_pane);
//     main_pane.add_pane(&view_pane);
//     left_pane.get_container().append_child(&PaneStyle::create("div", "panel", &PaneStyleOptions::default()));
//     main_pane.attach_to(&body);
// - 원본과 다른 점:
//   - PaneTheme 은 원본의 묶음(color, size, font)을 이름 앞머리로 편 연관 상수다. (PaneTheme.color.background → PaneTheme::COLOR_BACKGROUND,
//     PaneTheme.size.resizer → PaneTheme::SIZE_RESIZER, PaneTheme.font.sizeSmall → PaneTheme::FONT_SIZE_SMALL)
//     줄인 이름은 풀었다. (color.inputBg → COLOR_INPUT_BACKGROUND, font.mono → FONT_MONOSPACE) SIZE_RESIZER 는 픽셀 수(f64)다.
//   - PaneStyle::apply 의 스타일은 (CSS 속성 이름, 값) 목록이다. 이름은 원본 객체 키(backgroundColor)가 아니라 CSS 이름(background-color)이다.
//     (러스트에는 Object.assign 이 없어서 style.setProperty 로 넣는다)
//   - PaneStyle::create 의 옵션은 PaneStyleOptions 다. 빈 글은 원본의 빈 값처럼 넣지 않는다. 원본 options.type 은 element_type 이다. (type 은 러스트 예약어)
//     태그 이름이 틀리면 원본처럼 그 자리에서 멈춘다. (원본은 예외, 러스트 판은 panic)
//   - Pane 의 옵션은 PaneOptions 다. 크기는 PaneSize 다. (Flex, Fixed(픽셀), Ratio(0~1 비율))
//     원본은 숫자 하나로 0 이나 "flex" 는 flex, 1 이하는 비율, 1 을 넘으면 픽셀로 읽는다. ("50%" 는 Ratio(0.5)) 그래서 get_initial_size 는 PaneSize 를 돌려준다.
//   - Pane 은 상태를 Rc<RefCell> 로 나눠 드는 손잡이다. clone 은 같은 패널을 가리킨다. 자식은 부모를 Weak 로 가리킨다.
//     attach_to 가 단 창 resize 리스너가 뿌리 패널을 끝까지 붙든다. (원본과 같다) attach_to 하지 않은 패널은 손잡이를 모두 놓으면 풀리고, 리사이저를 끌어도 아무것도 하지 않는다.
//   - 리사이저 두께는 원본처럼 data-thickness 속성에 적되, 계산은 러스트 쪽에 든 값으로 한다.
//   - 리스너는 wasm-bindgen Closure 다. 요소에 다는 것은 into_js_value 로 JS 가비지 컬렉터에 넘기고, 요소는 사건의 currentTarget 으로 읽는다.
//     (요소를 잡아 두면 러스트 쪽이 요소를 붙들어 DOM 에서 빼도 풀리지 않는다)
//   - set_callback 은 None 으로 비울 수 있다. 콜백 안에서 패널을 다시 불러도 된다. (부르는 동안 빼 두었다가 돌려놓는다)
//==============================================================================
use std::cell::RefCell;
use std::rc::{Rc, Weak};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{Document, Element, Event, HtmlElement, MouseEvent, Window};

// 패널 크기가 다시 놓일 때 부르는 콜백. (원본 setCallback)
pub type PaneResizeCallback = Box<dyn FnMut()>;

//==============================================================================
// 테마 상수. (원본 PaneTheme)
//==============================================================================
pub struct PaneTheme;

impl PaneTheme {
	pub const COLOR_BACKGROUND: &'static str = "#1e1e1e";
	pub const COLOR_PANEL: &'static str = "#252526";
	pub const COLOR_TOOLBAR: &'static str = "#2d2d30";
	pub const COLOR_BORDER: &'static str = "#444";
	pub const COLOR_TEXT: &'static str = "#ccc";
	pub const COLOR_TEXT_DIM: &'static str = "#aaaaaa";
	pub const COLOR_ACCENT: &'static str = "#0e639c";
	pub const COLOR_ACCENT_HOVER: &'static str = "#1177bb";
	// 주황색.
	pub const COLOR_WARNING: &'static str = "#d97706";
	pub const COLOR_WARNING_HOVER: &'static str = "#f59e0b";
	pub const COLOR_ERROR: &'static str = "#f44336";
	pub const COLOR_SUCCESS: &'static str = "#4CAF50";
	pub const COLOR_INPUT_BACKGROUND: &'static str = "#3c3c3c";
	pub const COLOR_RESIZER: &'static str = "#333";
	pub const COLOR_RESIZER_HOVER: &'static str = "#007acc";
	// 리사이저 두께. (픽셀)
	pub const SIZE_RESIZER: f64 = 5.0;
	pub const FONT_FAMILY: &'static str = "sans-serif";
	pub const FONT_MONOSPACE: &'static str = "\"Consolas\", monospace";
	pub const FONT_SIZE: &'static str = "13px";
	pub const FONT_SIZE_SMALL: &'static str = "12px";
}

//==============================================================================
// PaneStyle::create 옵션. (원본 options: { id, text, style, type, placeholder, accent, warning })
//==============================================================================
#[derive(Clone, Copy, Debug, Default)]
pub struct PaneStyleOptions<'a> {
	pub id: &'a str,
	pub text: &'a str,
	pub style: &'a [(&'a str, &'a str)],
	pub element_type: &'a str,
	pub placeholder: &'a str,
	pub accent: bool,
	pub warning: bool,
}

//==============================================================================
// 스타일 및 요소 생성 유틸리티. (원본 PaneStyle)
//==============================================================================
pub struct PaneStyle;

impl PaneStyle {
	//==============================================================================
	// 요소에 여러 스타일을 한꺼번에 넣는다. ((CSS 속성 이름, 값) 목록)
	//==============================================================================
	pub fn apply(element: &HtmlElement, styles: &[(&str, &str)]) {
		let style = element.style();
		for (property_name, property_value) in styles {
			let _ = style.set_property(property_name, property_value);
		}
	}

	//==============================================================================
	// 스타일 프리셋을 넣은 요소를 만든다.
	// (preset: "panel" | "toolbar" | "button" | "input" | "textarea" | "text", 그 밖은 공통 스타일만)
	//==============================================================================
	pub fn create(tag_name: &str, preset: &str, options: &PaneStyleOptions) -> HtmlElement {
		let element = create_html_element(tag_name);
		if !options.id.is_empty() {
			element.set_id(options.id);
		}
		if !options.text.is_empty() {
			element.set_inner_text(options.text);
		}
		if !options.element_type.is_empty() {
			set_element_property(&element, "type", options.element_type);
		}
		if !options.placeholder.is_empty() {
			set_element_property(&element, "placeholder", options.placeholder);
		}

		// 공통 기본 스타일. (absolute 배치에 맞춘다)
		PaneStyle::apply(&element, &[
			("position", "absolute"),
			("left", "0"),
			("top", "0"),
			("width", "100%"),
			("height", "100%"),
			("box-sizing", "border-box"),
			("font-family", PaneTheme::FONT_FAMILY),
			("font-size", PaneTheme::FONT_SIZE),
			("color", PaneTheme::COLOR_TEXT),
			("margin", "0"),
			("padding", "0"),
		]);

		// 프리셋별 스타일.
		match preset {
			"toolbar" => {
				let border_bottom = format!("1px solid {}", PaneTheme::COLOR_BORDER);
				PaneStyle::apply(&element, &[
					("background-color", PaneTheme::COLOR_TOOLBAR),
					("border-bottom", &border_bottom),
					("display", "flex"),
					("justify-content", "center"),
					("align-items", "center"),
					("gap", "10px"),
				]);
			}
			"panel" => {
				PaneStyle::apply(&element, &[
					("background-color", PaneTheme::COLOR_PANEL),
					("padding", "10px"),
				]);
			}
			"button" => {
				let mut background_color = PaneTheme::COLOR_RESIZER;
				if options.accent {
					background_color = PaneTheme::COLOR_ACCENT;
				}
				if options.warning {
					background_color = PaneTheme::COLOR_WARNING;
				}
				PaneStyle::apply(&element, &[
					("position", "relative"),
					("width", "auto"),
					("height", "auto"),
					("padding", "6px 16px"),
					("border", "none"),
					("border-radius", "4px"),
					("cursor", "pointer"),
					("font-weight", "bold"),
					("background-color", background_color),
				]);
				let mouse_enter_callback: Closure<dyn FnMut(Event)> = Closure::new(move |event: Event| {
					if let Some(target_element) = read_current_target_element(&event) {
						set_style(&target_element, "filter", "brightness(1.2)");
					}
				});
				let mouse_leave_callback: Closure<dyn FnMut(Event)> = Closure::new(move |event: Event| {
					if let Some(target_element) = read_current_target_element(&event) {
						set_style(&target_element, "filter", "none");
					}
				});
				add_element_listener(&element, "mouseenter", mouse_enter_callback.into_js_value());
				add_element_listener(&element, "mouseleave", mouse_leave_callback.into_js_value());
			}
			"input" => {
				let border = format!("1px solid {}", PaneTheme::COLOR_RESIZER);
				PaneStyle::apply(&element, &[
					("position", "relative"),
					("height", "auto"),
					("background", PaneTheme::COLOR_INPUT_BACKGROUND),
					("border", &border),
					("padding", "4px"),
				]);
			}
			"textarea" => {
				PaneStyle::apply(&element, &[
					("background-color", PaneTheme::COLOR_BACKGROUND),
					("color", "#d4d4d4"),
					("font-family", PaneTheme::FONT_MONOSPACE),
					("font-size", "14px"),
					("padding", "15px"),
					("border", "none"),
					("resize", "none"),
					("outline", "none"),
					("tab-size", "4"),
				]);
			}
			"text" => {
				PaneStyle::apply(&element, &[
					("position", "relative"),
					("width", "auto"),
					("height", "auto"),
					("font-weight", "bold"),
				]);
			}
			_ => {
			}
		}

		// 덧붙이는 스타일.
		PaneStyle::apply(&element, options.style);

		element
	}
}

//==============================================================================
// 나누는 방향. (원본 "horizontal" | "vertical")
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneDirection {
	// 자식을 왼쪽에서 오른쪽으로 놓는다.
	#[default]
	Horizontal,
	// 자식을 위에서 아래로 놓는다.
	Vertical,
}

//==============================================================================
// 처음 크기. (부모의 나누는 방향 쪽 길이)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PaneSize {
	// 고정 크기 형제들이 쓰고 남은 자리를 flex 형제끼리 똑같이 나눈다. (원본 0, "flex")
	#[default]
	Flex,
	// 픽셀. (원본 1 을 넘는 숫자)
	Fixed(f64),
	// 리사이저를 뺀 부모 길이에 곱하는 비율. (원본 1 이하 숫자, "50%")
	Ratio(f64),
}

//==============================================================================
// 리사이저로 끌 수 있는 가장자리. (원본 resizableEdges)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneResizableEdges {
	pub top: bool,
	pub bottom: bool,
	pub left: bool,
	pub right: bool,
}

impl Default for PaneResizableEdges {
	//==============================================================================
	// 기본값. (네 가장자리 모두 끌 수 있다)
	//==============================================================================
	fn default() -> PaneResizableEdges {
		PaneResizableEdges {
			top: true,
			bottom: true,
			left: true,
			right: true,
		}
	}
}

//==============================================================================
// Pane 생성 옵션. (원본 options: { direction, size, minSize, maxSize, isResizable, resizableEdges })
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct PaneOptions {
	pub direction: PaneDirection,
	pub size: PaneSize,
	pub min_size: f64,
	pub max_size: f64,
	pub is_resizable: bool,
	pub resizable_edges: PaneResizableEdges,
}

impl Default for PaneOptions {
	//==============================================================================
	// 기본값. (원본과 같이 가로, flex, 최소 50, 최대 제한 없음, 끌 수 있음)
	//==============================================================================
	fn default() -> PaneOptions {
		PaneOptions {
			direction: PaneDirection::Horizontal,
			size: PaneSize::Flex,
			min_size: 50.0,
			max_size: f64::MAX,
			is_resizable: true,
			resizable_edges: PaneResizableEdges::default(),
		}
	}
}

//==============================================================================
// 자식 사이 리사이저 하나.
//==============================================================================
#[derive(Clone)]
struct PaneResizer {
	element: HtmlElement,
	thickness: f64,
}

//==============================================================================
// 패널 상태. (Pane 손잡이들과 DOM 리스너가 나눠 든다)
//==============================================================================
struct PaneState {
	container: HtmlElement,
	direction: PaneDirection,
	children: Vec<Pane>,
	resizer_list: Vec<PaneResizer>,

	initial_size: PaneSize,
	fixed_size: f64,
	resize_grab_offset: f64,
	min_size: f64,
	max_size: f64,

	is_resizable: bool,
	resizable_edges: PaneResizableEdges,
	is_visible: bool,

	resize_callback: Option<PaneResizeCallback>,
	parent_pane: Weak<RefCell<PaneState>>,

	// 리사이즈 끌기.
	is_resizing: bool,
	active_previous_pane: Option<Pane>,
	active_next_pane: Option<Pane>,
	mouse_move_callback: Option<Closure<dyn FnMut(MouseEvent)>>,
	mouse_up_callback: Option<Closure<dyn FnMut(MouseEvent)>>,
}

//==============================================================================
// 화면 분할 패널. (원본 Pane, clone 은 같은 패널을 가리킨다)
//==============================================================================
#[derive(Clone)]
pub struct Pane {
	state: Rc<RefCell<PaneState>>,
}

impl Pane {
	//==============================================================================
	// 생성. (컨테이너 div 를 만들고, 문서에 다는 끌기 리스너를 지어 둔다)
	//==============================================================================
	pub fn new(options: PaneOptions) -> Pane {
		let fixed_size = match options.size {
			PaneSize::Fixed(pixel_size) => {
				pixel_size
			}
			PaneSize::Flex | PaneSize::Ratio(_) => {
				0.0
			}
		};
		let container = create_html_element("div");
		PaneStyle::apply(&container, &[
			("position", "absolute"),
			("overflow", "hidden"),
			("box-sizing", "border-box"),
			("background-color", PaneTheme::COLOR_BACKGROUND),
		]);
		let state = Rc::new(RefCell::new(PaneState {
			container,
			direction: options.direction,
			children: Vec::new(),
			resizer_list: Vec::new(),
			initial_size: options.size,
			fixed_size,
			resize_grab_offset: 0.0,
			min_size: options.min_size,
			max_size: options.max_size,
			is_resizable: options.is_resizable,
			resizable_edges: options.resizable_edges,
			is_visible: true,
			resize_callback: None,
			parent_pane: Weak::new(),
			is_resizing: false,
			active_previous_pane: None,
			active_next_pane: None,
			mouse_move_callback: None,
			mouse_up_callback: None,
		}));

		let mouse_move_state = Rc::downgrade(&state);
		let mouse_move_callback: Closure<dyn FnMut(MouseEvent)> = Closure::new(move |mouse_event: MouseEvent| {
			if let Some(state) = mouse_move_state.upgrade() {
				Pane { state }.handle_mouse_move(&mouse_event);
			}
		});
		let mouse_up_state = Rc::downgrade(&state);
		let mouse_up_callback: Closure<dyn FnMut(MouseEvent)> = Closure::new(move |_mouse_event: MouseEvent| {
			if let Some(state) = mouse_up_state.upgrade() {
				Pane { state }.handle_mouse_up();
			}
		});
		{
			let mut state_mut = state.borrow_mut();
			state_mut.mouse_move_callback = Some(mouse_move_callback);
			state_mut.mouse_up_callback = Some(mouse_up_callback);
		}
		Pane { state }
	}

	//==============================================================================
	// 자식 추가. (앞 자식이 있으면 그 사이에 리사이저를 둔다)
	//==============================================================================
	pub fn add_pane(&self, pane: &Pane) {
		pane.state.borrow_mut().parent_pane = Rc::downgrade(&self.state);

		let previous_pane = self.state.borrow().children.last().cloned();
		if let Some(previous_pane) = previous_pane {
			let resizer = self.create_resizer(&previous_pane, pane);
			let mut state = self.state.borrow_mut();
			let _ = state.container.append_child(&resizer.element);
			state.resizer_list.push(resizer);
		}

		let pane_container = pane.get_container();
		let mut state = self.state.borrow_mut();
		state.children.push(pane.clone());
		let _ = state.container.append_child(&pane_container);
	}

	//==============================================================================
	// 두 자식 사이 리사이저를 만든다. (끌 수 없으면 두께 1 의 선이다)
	//==============================================================================
	fn create_resizer(&self, previous_pane: &Pane, next_pane: &Pane) -> PaneResizer {
		let (is_resizable, direction) = {
			let state = self.state.borrow();
			(state.is_resizable, state.direction)
		};
		let resizer = create_html_element("div");
		PaneStyle::apply(&resizer, &[
			("position", "absolute"),
			("background-color", PaneTheme::COLOR_RESIZER),
			("z-index", "1000"),
		]);

		// 끌 수 있는지 본다.
		let previous_edges = previous_pane.get_resizable_edges();
		let next_edges = next_pane.get_resizable_edges();
		let mut can_resize = is_resizable && previous_pane.is_resizable() && next_pane.is_resizable();
		match direction {
			PaneDirection::Horizontal => {
				if !previous_edges.right || !next_edges.left {
					can_resize = false;
				}
			}
			PaneDirection::Vertical => {
				if !previous_edges.bottom || !next_edges.top {
					can_resize = false;
				}
			}
		}

		let thickness = if can_resize {
			PaneTheme::SIZE_RESIZER
		}
		else {
			1.0
		};
		let thickness_text = format!("{}px", thickness);
		match direction {
			PaneDirection::Horizontal => {
				set_style(&resizer, "width", &thickness_text);
				if can_resize {
					set_style(&resizer, "cursor", "col-resize");
				}
			}
			PaneDirection::Vertical => {
				set_style(&resizer, "height", &thickness_text);
				if can_resize {
					set_style(&resizer, "cursor", "row-resize");
				}
			}
		}

		if can_resize {
			let mouse_enter_callback: Closure<dyn FnMut(Event)> = Closure::new(move |event: Event| {
				if let Some(resizer_element) = read_current_target_element(&event) {
					set_style(&resizer_element, "background-color", PaneTheme::COLOR_RESIZER_HOVER);
				}
			});
			let mouse_leave_state = Rc::downgrade(&self.state);
			let mouse_leave_callback: Closure<dyn FnMut(Event)> = Closure::new(move |event: Event| {
				let is_resizing = match mouse_leave_state.upgrade() {
					Some(state) => {
						state.borrow().is_resizing
					}
					None => {
						false
					}
				};
				if is_resizing {
					return;
				}
				if let Some(resizer_element) = read_current_target_element(&event) {
					set_style(&resizer_element, "background-color", PaneTheme::COLOR_RESIZER);
				}
			});
			let mouse_down_state = Rc::downgrade(&self.state);
			let mouse_down_previous_state = Rc::downgrade(&previous_pane.state);
			let mouse_down_next_state = Rc::downgrade(&next_pane.state);
			let mouse_down_callback: Closure<dyn FnMut(MouseEvent)> = Closure::new(move |mouse_event: MouseEvent| {
				let state = match mouse_down_state.upgrade() {
					Some(state) => {
						state
					}
					None => {
						return;
					}
				};
				let previous_state = match mouse_down_previous_state.upgrade() {
					Some(previous_state) => {
						previous_state
					}
					None => {
						return;
					}
				};
				let next_state = match mouse_down_next_state.upgrade() {
					Some(next_state) => {
						next_state
					}
					None => {
						return;
					}
				};
				let resizer_element = match read_current_target_element(&mouse_event) {
					Some(resizer_element) => {
						resizer_element
					}
					None => {
						return;
					}
				};
				// 첫 이동에서 레이아웃이 바뀌며 누른 자리에 draggable 요소가 오면
				// 브라우저가 그 요소의 끌기를 시작해 mousemove/mouseup 을 삼킨다. 기본 동작을 막는다.
				mouse_event.prevent_default();
				Pane { state }.begin_resize(Pane { state: previous_state }, Pane { state: next_state }, &resizer_element, &mouse_event);
			});
			add_element_listener(&resizer, "mouseenter", mouse_enter_callback.into_js_value());
			add_element_listener(&resizer, "mouseleave", mouse_leave_callback.into_js_value());
			add_element_listener(&resizer, "mousedown", mouse_down_callback.into_js_value());
		}

		let _ = resizer.set_attribute("data-thickness", &thickness.to_string());
		PaneResizer {
			element: resizer,
			thickness,
		}
	}

	//==============================================================================
	// 리사이저 끌기 시작. (잡은 자리를 적고 문서에 이동, 뗌 리스너를 단다)
	//==============================================================================
	fn begin_resize(&self, previous_pane: Pane, next_pane: Pane, resizer_element: &HtmlElement, mouse_event: &MouseEvent) {
		let mut state = self.state.borrow_mut();
		state.is_resizing = true;
		state.active_previous_pane = Some(previous_pane);
		state.active_next_pane = Some(next_pane);

		// 잡은 지점과 경계 사이 간격을 기억해 두어야 처음 끌 때 크기가 튀지 않는다.
		let grab_rect = resizer_element.get_bounding_client_rect();
		state.resize_grab_offset = match state.direction {
			PaneDirection::Horizontal => {
				mouse_event.client_x() as f64 - grab_rect.left()
			}
			PaneDirection::Vertical => {
				mouse_event.client_y() as f64 - grab_rect.top()
			}
		};
		set_style(resizer_element, "background-color", PaneTheme::COLOR_RESIZER_HOVER);
		let document = read_document();
		if let Some(body) = document.body() {
			let cursor = resizer_element.style().get_property_value("cursor").unwrap_or_default();
			set_style(&body, "cursor", &cursor);
			set_style(&body, "user-select", "none");
		}
		if let Some(mouse_move_callback) = state.mouse_move_callback.as_ref() {
			let _ = document.add_event_listener_with_callback("mousemove", mouse_move_callback.as_ref().unchecked_ref());
		}
		if let Some(mouse_up_callback) = state.mouse_up_callback.as_ref() {
			let _ = document.add_event_listener_with_callback("mouseup", mouse_up_callback.as_ref().unchecked_ref());
		}
	}

	//==============================================================================
	// 마우스 이동. (끄는 중이면 앞 자식이나 뒤 자식의 고정 크기를 바꾸고 뿌리부터 다시 놓는다)
	//==============================================================================
	fn handle_mouse_move(&self, mouse_event: &MouseEvent) {
		let (direction, previous_pane, next_pane, thickness, resize_grab_offset) = {
			let state = self.state.borrow();
			if !state.is_resizing {
				return;
			}
			let (previous_pane, next_pane) = match (state.active_previous_pane.as_ref(), state.active_next_pane.as_ref()) {
				(Some(previous_pane), Some(next_pane)) => {
					(previous_pane.clone(), next_pane.clone())
				}
				_ => {
					return;
				}
			};
			let next_index = match state.children.iter().position(|child| Rc::ptr_eq(&child.state, &next_pane.state)) {
				Some(next_index) => {
					next_index
				}
				None => {
					return;
				}
			};
			let thickness = state.resizer_list[next_index - 1].thickness;
			(state.direction, previous_pane, next_pane, thickness, state.resize_grab_offset)
		};

		let previous_rect = previous_pane.get_container().get_bounding_client_rect();
		let next_rect = next_pane.get_container().get_bounding_client_rect();
		let (total_available, mut new_previous_size) = match direction {
			PaneDirection::Horizontal => {
				((next_rect.right() - previous_rect.left()) - thickness, (mouse_event.client_x() as f64 - resize_grab_offset) - previous_rect.left())
			}
			PaneDirection::Vertical => {
				((next_rect.bottom() - previous_rect.top()) - thickness, (mouse_event.client_y() as f64 - resize_grab_offset) - previous_rect.top())
			}
		};
		new_previous_size = f64::max(previous_pane.get_min_size(), f64::min(new_previous_size, total_available - next_pane.get_min_size()));

		if previous_pane.is_flex() && !next_pane.is_flex() {
			next_pane.set_fixed_size(total_available - new_previous_size);
		}
		else {
			previous_pane.set_fixed_size(new_previous_size);
		}

		self.find_root_pane().refresh();
	}

	//==============================================================================
	// 마우스 뗌. (끌기를 끝내고 문서 리스너를 뗀다)
	//==============================================================================
	fn handle_mouse_up(&self) {
		let mut state = self.state.borrow_mut();
		state.is_resizing = false;
		let document = read_document();
		if let Some(body) = document.body() {
			set_style(&body, "cursor", "default");
			set_style(&body, "user-select", "auto");
		}
		if let Some(mouse_move_callback) = state.mouse_move_callback.as_ref() {
			let _ = document.remove_event_listener_with_callback("mousemove", mouse_move_callback.as_ref().unchecked_ref());
		}
		if let Some(mouse_up_callback) = state.mouse_up_callback.as_ref() {
			let _ = document.remove_event_listener_with_callback("mouseup", mouse_up_callback.as_ref().unchecked_ref());
		}
		for resizer in state.resizer_list.iter() {
			set_style(&resizer.element, "background-color", PaneTheme::COLOR_RESIZER);
		}
	}

	//==============================================================================
	// 배치. (자기 자리를 놓고, 고정 크기 자식을 먼저 재고 남은 자리를 flex 자식끼리 나눈 뒤 자식과 리사이저를 차례로 놓는다)
	//==============================================================================
	pub fn update_layout(&self, x: f64, y: f64, width: f64, height: f64) {
		let (container, direction, children, resizer_list) = {
			let state = self.state.borrow();
			(state.container.clone(), state.direction, state.children.clone(), state.resizer_list.clone())
		};
		PaneStyle::apply(&container, &[
			("left", &format!("{}px", x)),
			("top", &format!("{}px", y)),
			("width", &format!("{}px", width)),
			("height", &format!("{}px", height)),
		]);

		if children.is_empty() {
			return;
		}

		// 숨긴 자식과 맞닿은 리사이저는 숨기고 두께에서 뺀다.
		let mut total_resizer_thickness = 0.0;
		let mut resizer_visible_list: Vec<bool> = Vec::with_capacity(resizer_list.len());
		for (index, resizer) in resizer_list.iter().enumerate() {
			let is_resizer_visible = children[index].is_visible() && children[index + 1].is_visible();
			if is_resizer_visible {
				set_style(&resizer.element, "display", "");
				total_resizer_thickness += resizer.thickness;
			}
			else {
				set_style(&resizer.element, "display", "none");
			}
			resizer_visible_list.push(is_resizer_visible);
		}

		let main_size = match direction {
			PaneDirection::Horizontal => {
				width
			}
			PaneDirection::Vertical => {
				height
			}
		};
		let available_size = main_size - total_resizer_thickness;

		let mut used_size = 0.0;
		let mut flex_count = 0;
		let mut last_visible_index: Option<usize> = None;
		let mut size_list: Vec<f64> = vec![0.0; children.len()];
		for (index, child) in children.iter().enumerate() {
			if !child.is_visible() {
				continue;
			}
			last_visible_index = Some(index);
			if child.is_flex() {
				flex_count += 1;
			}
			else {
				let mut size = child.get_fixed_size();
				if size == 0.0 {
					size = match child.get_initial_size() {
						PaneSize::Ratio(ratio) => {
							available_size * ratio
						}
						PaneSize::Fixed(pixel_size) => {
							pixel_size
						}
						PaneSize::Flex => {
							0.0
						}
					};
				}
				size = f64::max(child.get_min_size(), f64::min(size, child.get_max_size()));
				size_list[index] = size;
				used_size += size;
			}
		}

		let size_per_flex = if flex_count > 0 {
			f64::max(0.0, available_size - used_size) / flex_count as f64
		}
		else {
			0.0
		};
		for (index, child) in children.iter().enumerate() {
			if child.is_visible() && child.is_flex() {
				size_list[index] = size_per_flex;
			}
		}

		let mut offset = 0.0;
		for (index, child) in children.iter().enumerate() {
			if !child.is_visible() {
				continue;
			}
			let child_size = if Some(index) == last_visible_index {
				main_size - offset
			}
			else {
				size_list[index]
			};

			match direction {
				PaneDirection::Horizontal => {
					child.update_layout(offset, 0.0, child_size, height);
				}
				PaneDirection::Vertical => {
					child.update_layout(0.0, offset, width, child_size);
				}
			}

			offset += child_size;
			if index < resizer_list.len() && resizer_visible_list[index] {
				let resizer = &resizer_list[index];
				let offset_text = format!("{}px", offset);
				let thickness_text = format!("{}px", resizer.thickness);
				match direction {
					PaneDirection::Horizontal => {
						let height_text = format!("{}px", height);
						PaneStyle::apply(&resizer.element, &[
							("left", &offset_text),
							("top", "0px"),
							("width", &thickness_text),
							("height", &height_text),
						]);
					}
					PaneDirection::Vertical => {
						let width_text = format!("{}px", width);
						PaneStyle::apply(&resizer.element, &[
							("left", "0px"),
							("top", &offset_text),
							("width", &width_text),
							("height", &thickness_text),
						]);
					}
				}
				offset += resizer.thickness;
			}
		}

		// 콜백 안에서 이 패널을 다시 불러도 되게 빼 두었다가 돌려놓는다. (콜백이 새 콜백을 걸었으면 그것을 둔다)
		let resize_callback = self.state.borrow_mut().resize_callback.take();
		if let Some(mut resize_callback) = resize_callback {
			resize_callback();
			let mut state = self.state.borrow_mut();
			if state.resize_callback.is_none() {
				state.resize_callback = Some(resize_callback);
			}
		}
	}

	//==============================================================================
	// 배치 초기화. (끌어서 바꾼 크기를 지운다, 자식까지)
	//==============================================================================
	pub fn reset_layout(&self) {
		let children = {
			let mut state = self.state.borrow_mut();
			state.fixed_size = 0.0;
			state.children.clone()
		};
		for child in children.iter() {
			child.reset_layout();
		}
	}

	//==============================================================================
	// 부모 요소 크기에 맞춰 다시 놓는다. (부모 크기가 0 이면 창 크기, 부모가 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn refresh(&self) {
		let parent_element = match self.get_container().parent_element() {
			Some(parent_element) => {
				parent_element
			}
			None => {
				return;
			}
		};
		let window = read_window();
		let width = if parent_element.client_width() != 0 {
			parent_element.client_width() as f64
		}
		else {
			window.inner_width().ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
		};
		let height = if parent_element.client_height() != 0 {
			parent_element.client_height() as f64
		}
		else {
			window.inner_height().ok().and_then(|value| value.as_f64()).unwrap_or(0.0)
		};
		self.update_layout(0.0, 0.0, width, height);
	}

	//==============================================================================
	// 요소에 단다. (창 크기가 바뀔 때와 다음 프레임에 다시 놓는다. 창 리스너가 이 패널을 끝까지 붙든다)
	//==============================================================================
	pub fn attach_to(&self, parent_element: &Element) {
		let _ = parent_element.append_child(&self.get_container());
		let window = read_window();
		let resize_pane = self.clone();
		let resize_callback: Closure<dyn FnMut()> = Closure::new(move || {
			resize_pane.refresh();
		});
		let _ = window.add_event_listener_with_callback("resize", resize_callback.as_ref().unchecked_ref());
		resize_callback.forget();
		self.refresh();
		let frame_pane = self.clone();
		let frame_callback = Closure::once_into_js(move || {
			frame_pane.refresh();
		});
		let _ = window.request_animation_frame(frame_callback.unchecked_ref());
	}

	//==============================================================================
	// 보이기 / 숨기기. (숨긴 패널은 자리를 차지하지 않고, 맞닿은 리사이저도 숨는다)
	//==============================================================================
	pub fn set_visible(&self, is_visible: bool) {
		{
			let mut state = self.state.borrow_mut();
			state.is_visible = is_visible;
			let display = if is_visible {
				""
			}
			else {
				"none"
			};
			set_style(&state.container, "display", display);
		}
		self.find_root_pane().refresh();
	}

	//==============================================================================
	// 뿌리 패널. (부모를 따라 끝까지 올라간다)
	//==============================================================================
	fn find_root_pane(&self) -> Pane {
		let mut root_pane = self.clone();
		loop {
			let parent_state = root_pane.state.borrow().parent_pane.upgrade();
			match parent_state {
				Some(parent_state) => {
					root_pane = Pane { state: parent_state };
				}
				None => {
					return root_pane;
				}
			}
		}
	}

	//==============================================================================
	// 보이는지.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.state.borrow().is_visible
	}

	//==============================================================================
	// flex 인지. (처음 크기가 Flex)
	//==============================================================================
	pub fn is_flex(&self) -> bool {
		self.state.borrow().initial_size == PaneSize::Flex
	}

	//==============================================================================
	// 처음 크기.
	//==============================================================================
	pub fn get_initial_size(&self) -> PaneSize {
		self.state.borrow().initial_size
	}

	//==============================================================================
	// 고정 크기. (픽셀, 0 이면 처음 크기로 잰다)
	//==============================================================================
	pub fn get_fixed_size(&self) -> f64 {
		self.state.borrow().fixed_size
	}

	//==============================================================================
	// 고정 크기 설정. (다음 배치부터 쓴다. flex 패널에는 쓰이지 않는다)
	//==============================================================================
	pub fn set_fixed_size(&self, fixed_size: f64) {
		self.state.borrow_mut().fixed_size = fixed_size;
	}

	//==============================================================================
	// 최소 크기.
	//==============================================================================
	pub fn get_min_size(&self) -> f64 {
		self.state.borrow().min_size
	}

	//==============================================================================
	// 최대 크기.
	//==============================================================================
	pub fn get_max_size(&self) -> f64 {
		self.state.borrow().max_size
	}

	//==============================================================================
	// 컨테이너 요소. (내용을 여기에 붙인다)
	//==============================================================================
	pub fn get_container(&self) -> HtmlElement {
		self.state.borrow().container.clone()
	}

	//==============================================================================
	// 배치 콜백 설정. (update_layout 이 끝날 때마다 부른다. None 이면 비운다)
	//==============================================================================
	pub fn set_callback(&self, resize_callback: Option<PaneResizeCallback>) {
		self.state.borrow_mut().resize_callback = resize_callback;
	}

	//==============================================================================
	// 리사이저로 끌 수 있는지.
	//==============================================================================
	pub fn is_resizable(&self) -> bool {
		self.state.borrow().is_resizable
	}

	//==============================================================================
	// 끌 수 있는 가장자리.
	//==============================================================================
	pub fn get_resizable_edges(&self) -> PaneResizableEdges {
		self.state.borrow().resizable_edges
	}
}

//==============================================================================
// 브라우저 창. (웹 빌드에는 늘 있다)
//==============================================================================
fn read_window() -> Window {
	web_sys::window().expect("Pane: window 없음")
}

//==============================================================================
// 문서. (웹 빌드에는 늘 있다)
//==============================================================================
fn read_document() -> Document {
	read_window().document().expect("Pane: document 없음")
}

//==============================================================================
// HTML 요소 생성. (태그 이름이 틀리면 원본의 예외처럼 멈춘다)
//==============================================================================
fn create_html_element(tag_name: &str) -> HtmlElement {
	let element = read_document().create_element(tag_name).expect("Pane: 요소를 만들 수 없는 태그 이름");
	element.dyn_into::<HtmlElement>().expect("Pane: HTML 요소가 아님")
}

//==============================================================================
// 스타일 하나 넣기. (빈 값이면 그 속성을 지운다)
//==============================================================================
fn set_style(element: &HtmlElement, property_name: &str, property_value: &str) {
	let _ = element.style().set_property(property_name, property_value);
}

//==============================================================================
// 요소의 JS 속성 하나 넣기. (원본이 요소의 type, placeholder 에 바로 넣는 것)
//==============================================================================
fn set_element_property(element: &HtmlElement, property_name: &str, property_value: &str) {
	let _ = js_sys::Reflect::set(element, &JsValue::from_str(property_name), &JsValue::from_str(property_value));
}

//==============================================================================
// 요소에 리스너 달기. (into_js_value 로 넘긴 함수)
//==============================================================================
fn add_element_listener(element: &HtmlElement, event_name: &str, listener: JsValue) {
	let _ = element.add_event_listener_with_callback(event_name, listener.unchecked_ref());
}

//==============================================================================
// 사건의 currentTarget 요소. (리스너를 단 요소)
//==============================================================================
fn read_current_target_element(event: &Event) -> Option<HtmlElement> {
	event.current_target()?.dyn_into::<HtmlElement>().ok()
}
