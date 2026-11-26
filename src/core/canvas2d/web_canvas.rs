//==============================================================================
//! 2D 그리기 대상의 웹 판. (원본처럼 문서에 붙이지 않은 브라우저 canvas 요소와 그 2D 컨텍스트에 그린다, core/canvas2d.rs 머리 주석)
//! - Path2D 는 엔진이 든 조각을 브라우저 Path2D 로 다시 그어 넘긴다. 그라데이션과 패턴은 칠할 것으로 정할 때 브라우저 것을 만든다.
//! - getter 는 브라우저 컨텍스트의 값을 읽는다. 변환과 칠할 것은 러스트 쪽에 같은 값을 들고 save, restore 와 함께 쌓는다.
//!
//! # 예제
//! 다른 캔버스의 두 픽셀을 매끄럽게 하지 않고 늘여 그리기
//! ```rust
//! use vanilla::Canvas2D;
//!
//! let mut source = Canvas2D::new(2, 1);
//! source.set_fill_style("#ff0000");
//! source.fill_rect(0.0, 0.0, 1.0, 1.0);
//! source.set_fill_style("#0000ff");
//! source.fill_rect(1.0, 0.0, 1.0, 1.0);
//!
//! let mut canvas = Canvas2D::new(8, 4);
//! canvas.set_image_smoothing_enabled(false);
//! canvas.draw_image_scaled(&source, 0.0, 0.0, 8.0, 4.0);
//! assert_eq!(canvas.get_image_data(1, 1, 1, 1).data, vec![255, 0, 0, 255]);
//! assert_eq!(canvas.get_image_data(6, 1, 1, 1).data, vec![0, 0, 255, 255]);
//! ```
//==============================================================================
use super::{is_composite_operation_name, normalize_line_dash, premultiply_pixels, unpremultiply_pixels, CanvasFillRule, CanvasGradient, CanvasGradientKind, CanvasImageSource, CanvasLineCap, CanvasLineJoin, CanvasMatrix, CanvasPattern, CanvasPatternRepetition, CanvasRadius, CanvasStyle, CanvasTextAlign, CanvasTextBaseline, ImageData, Path2D, BrowserPathCommand};
use crate::base::color::Color;
use crate::core::image::Image;
use crate::core::textstringtexturecache::TextMetrics;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, CanvasWindingRule, HtmlCanvasElement};

//==============================================================================
/// 러스트 쪽에 든 상태. (save / restore 와 함께 쌓는다)
//==============================================================================
#[derive(Clone)]
struct MirrorState {
	transform: CanvasMatrix,
	fill_style: CanvasStyle,
	stroke_style: CanvasStyle,
}

//==============================================================================
/// 2D 그리기 대상. (원본 캔버스와 그 2D 컨텍스트)
///
/// # 예제
/// 빨간 사각형을 칠하고, 상태를 쌓아 옮긴 자리에 반투명 파랑을 칠하기
/// ```rust
/// use vanilla::Canvas2D;
///
/// let mut canvas = Canvas2D::new(20, 10);
/// canvas.set_fill_style("#ff0000");
/// canvas.fill_rect(0.0, 0.0, 10.0, 10.0);
/// assert_eq!(canvas.get_image_data(5, 5, 1, 1).data, vec![255, 0, 0, 255]);
///
/// canvas.save();
/// canvas.translate(10.0, 0.0);
/// canvas.set_global_alpha(0.5);
/// canvas.set_fill_style("#0000ff");
/// canvas.fill_rect(0.0, 0.0, 10.0, 10.0);
/// canvas.restore();
/// assert_eq!(canvas.get_global_alpha(), 1.0);
/// assert_eq!(canvas.get_image_data(15, 5, 1, 1).data, vec![0, 0, 255, 128]);
/// ```
//==============================================================================
pub struct Canvas2D {
	width: u32,
	height: u32,
	canvas_element: HtmlCanvasElement,
	context: CanvasRenderingContext2d,
	state: MirrorState,
	state_stack: Vec<MirrorState>,
	revision: u64,
	cached_image: RefCell<Option<(u64, Rc<Image>)>>,
}

impl Canvas2D {
	//==============================================================================
	/// 만들기. (원본 document.createElement("canvas"), 문서에 붙이지 않는다)
	///
	/// # 인자
	/// - `width`: 폭. (픽셀)
	/// - `height`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 투명한 검정으로 채운 새 캔버스.
	//==============================================================================
	pub fn new(width: u32, height: u32) -> Canvas2D {
		let document = web_sys::window().and_then(|window| window.document()).expect("document 없음");
		let canvas_element: HtmlCanvasElement = document.create_element("canvas").expect("canvas 요소를 만들지 못했다").dyn_into().expect("canvas 요소가 아님");
		canvas_element.set_width(width);
		canvas_element.set_height(height);
		let context: CanvasRenderingContext2d = canvas_element.get_context("2d").ok().flatten().expect("2D 컨텍스트 없음").dyn_into().expect("2D 컨텍스트가 아님");
		Canvas2D {
			width,
			height,
			canvas_element,
			context,
			state: create_initial_state(),
			state_stack: Vec::new(),
			revision: 0,
			cached_image: RefCell::new(None),
		}
	}

	//==============================================================================
	/// 브라우저 canvas 요소. (웹만)
	///
	/// # 돌려줌
	/// 문서에 붙이지 않은 canvas 요소.
	//==============================================================================
	pub fn get_canvas_element(&self) -> &HtmlCanvasElement {
		&self.canvas_element
	}

	//==============================================================================
	/// 폭.
	///
	/// # 돌려줌
	/// 폭. (픽셀)
	//==============================================================================
	pub fn get_width(&self) -> u32 {
		self.width
	}

	//==============================================================================
	/// 높이.
	///
	/// # 돌려줌
	/// 높이. (픽셀)
	//==============================================================================
	pub fn get_height(&self) -> u32 {
		self.height
	}

	//==============================================================================
	/// 폭 설정. (원본처럼 그림을 비우고 상태를 처음으로 돌린다, 같은 값이어도)
	///
	/// # 인자
	/// - `width`: 새 폭. (픽셀)
	//==============================================================================
	pub fn set_width(&mut self, width: u32) {
		self.width = width;
		self.canvas_element.set_width(width);
		self.reset_mirror();
	}

	//==============================================================================
	/// 높이 설정. (원본처럼 그림을 비우고 상태를 처음으로 돌린다, 같은 값이어도)
	///
	/// # 인자
	/// - `height`: 새 높이. (픽셀)
	//==============================================================================
	pub fn set_height(&mut self, height: u32) {
		self.height = height;
		self.canvas_element.set_height(height);
		self.reset_mirror();
	}

	//==============================================================================
	/// 러스트 쪽 상태를 처음으로.
	//==============================================================================
	fn reset_mirror(&mut self) {
		self.state = create_initial_state();
		self.state_stack.clear();
		self.revision += 1;
	}

	//==============================================================================
	/// 처음으로 돌리기. (원본 reset, 같은 크기를 다시 넣으면 브라우저가 그림과 상태를 비운다)
	//==============================================================================
	pub fn reset(&mut self) {
		self.canvas_element.set_width(self.width);
		self.reset_mirror();
	}

	//==============================================================================
	/// 상태 쌓기. (원본 save)
	//==============================================================================
	pub fn save(&mut self) {
		self.context.save();
		self.state_stack.push(self.state.clone());
	}

	//==============================================================================
	/// 상태 꺼내기. (원본 restore, 쌓은 것이 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn restore(&mut self) {
		self.context.restore();
		if let Some(state) = self.state_stack.pop() {
			self.state = state;
		}
	}

	//==============================================================================
	/// 늘이기. (원본 scale)
	///
	/// # 인자
	/// - `x`: 가로 배율.
	/// - `y`: 세로 배율.
	//==============================================================================
	pub fn scale(&mut self, x: f32, y: f32) {
		self.transform(x, 0.0, 0.0, y, 0.0, 0.0);
	}

	//==============================================================================
	/// 돌리기. (원본 rotate, 라디안. 양수는 화면에서 시계 방향)
	///
	/// # 인자
	/// - `angle`: 돌릴 각. (라디안, 화면에서 시계 방향이 +)
	//==============================================================================
	pub fn rotate(&mut self, angle: f32) {
		self.transform(angle.cos(), angle.sin(), -angle.sin(), angle.cos(), 0.0, 0.0);
	}

	//==============================================================================
	/// 옮기기. (원본 translate)
	///
	/// # 인자
	/// - `x`: 가로 이동. (지금 좌표계 단위, 픽셀)
	/// - `y`: 세로 이동. (지금 좌표계 단위, 픽셀)
	//==============================================================================
	pub fn translate(&mut self, x: f32, y: f32) {
		self.transform(1.0, 0.0, 0.0, 1.0, x, y);
	}

	//==============================================================================
	/// 변환 곱하기. (원본 transform, 유한하지 않은 값이면 무시)
	///
	/// # 인자
	/// - `a`: 가로 배율. (x 에 곱해 새 x 에 더한다)
	/// - `b`: 세로 기울임. (x 에 곱해 새 y 에 더한다)
	/// - `c`: 가로 기울임. (y 에 곱해 새 x 에 더한다)
	/// - `d`: 세로 배율. (y 에 곱해 새 y 에 더한다)
	/// - `e`: 가로 이동. (지금 좌표계 단위, 픽셀)
	/// - `f`: 세로 이동. (지금 좌표계 단위, 픽셀)
	//==============================================================================
	pub fn transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
		let matrix = CanvasMatrix::create(a, b, c, d, e, f);
		if !matrix.is_finite() {
			return;
		}
		self.state.transform = self.state.transform.multiply(&matrix);
		self.apply_transform();
	}

	//==============================================================================
	/// 변환 바꾸기. (원본 setTransform(a, b, c, d, e, f))
	///
	/// # 인자
	/// - `a`: 가로 배율. (x 에 곱해 새 x 에 더한다)
	/// - `b`: 세로 기울임. (x 에 곱해 새 y 에 더한다)
	/// - `c`: 가로 기울임. (y 에 곱해 새 x 에 더한다)
	/// - `d`: 세로 배율. (y 에 곱해 새 y 에 더한다)
	/// - `e`: 가로 이동. (픽셀)
	/// - `f`: 세로 이동. (픽셀)
	//==============================================================================
	pub fn set_transform(&mut self, a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) {
		self.set_transform_matrix(CanvasMatrix::create(a, b, c, d, e, f));
	}

	//==============================================================================
	/// 변환 바꾸기. (원본 setTransform(DOMMatrix))
	///
	/// # 인자
	/// - `matrix`: 새 변환. (유한하지 않은 값이 있으면 무시)
	//==============================================================================
	pub fn set_transform_matrix(&mut self, matrix: CanvasMatrix) {
		if matrix.is_finite() {
			self.state.transform = matrix;
			self.apply_transform();
		}
	}

	//==============================================================================
	/// 변환 지우기. (원본 resetTransform)
	//==============================================================================
	pub fn reset_transform(&mut self) {
		self.state.transform = CanvasMatrix::IDENTITY;
		self.apply_transform();
	}

	//==============================================================================
	/// 러스트 쪽 변환을 브라우저에 넣기.
	//==============================================================================
	fn apply_transform(&self) {
		let matrix = self.state.transform;
		let _ = self.context.set_transform(matrix.a as f64, matrix.b as f64, matrix.c as f64, matrix.d as f64, matrix.e as f64, matrix.f as f64);
	}

	//==============================================================================
	/// 지금 변환. (원본 getTransform)
	///
	/// # 돌려줌
	/// 지금 변환 행렬. (처음 단위 행렬)
	//==============================================================================
	pub fn get_transform(&self) -> CanvasMatrix {
		self.state.transform
	}

	//==============================================================================
	/// 전체 투명도 설정. (원본 globalAlpha, 0 ~ 1 밖이나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `global_alpha`: 전체 투명도. (0 이 투명, 1 이 불투명, 처음 1)
	//==============================================================================
	pub fn set_global_alpha(&mut self, global_alpha: f32) {
		self.context.set_global_alpha(global_alpha as f64);
	}

	//==============================================================================
	/// 전체 투명도.
	///
	/// # 돌려줌
	/// 전체 투명도. (0 ~ 1, 처음 1)
	//==============================================================================
	pub fn get_global_alpha(&self) -> f32 {
		self.context.global_alpha() as f32
	}

	//==============================================================================
	/// 합치기 설정. (원본 globalCompositeOperation, 모르는 이름은 무시)
	///
	/// # 인자
	/// - `operation_name`: 원본 globalCompositeOperation 이름. ("multiply" 처럼 소문자와 하이픈, 대소문자를 가린다)
	//==============================================================================
	pub fn set_global_composite_operation(&mut self, operation_name: &str) {
		if is_composite_operation_name(operation_name) {
			let _ = self.context.set_global_composite_operation(operation_name);
		}
	}

	//==============================================================================
	/// 합치기.
	///
	/// # 돌려줌
	/// 원본 globalCompositeOperation 이름. (처음 "source-over")
	//==============================================================================
	pub fn get_global_composite_operation(&self) -> String {
		self.context.global_composite_operation().unwrap_or_else(|_| String::from("source-over"))
	}

	//==============================================================================
	/// 그림 매끄럽게 설정. (원본 imageSmoothingEnabled, 끄면 가장 가까운 픽셀)
	///
	/// # 인자
	/// - `is_enabled`: true 면 매끄럽게, false 면 가장 가까운 픽셀로 그린다. (처음 true)
	//==============================================================================
	pub fn set_image_smoothing_enabled(&mut self, is_enabled: bool) {
		self.context.set_image_smoothing_enabled(is_enabled);
	}

	//==============================================================================
	/// 그림 매끄럽게 하는지.
	///
	/// # 돌려줌
	/// 매끄럽게 그리면 true. (처음 true)
	//==============================================================================
	pub fn is_image_smoothing_enabled(&self) -> bool {
		self.context.image_smoothing_enabled()
	}

	//==============================================================================
	/// 채울 것 설정. (원본 fillStyle)
	///
	/// # 인자
	/// - `style`: 채울 것. (Color, "#rrggbb" 같은 CSS 색 글, CanvasGradient, CanvasPattern, 처음 검정)
	//==============================================================================
	pub fn set_fill_style(&mut self, style: impl Into<CanvasStyle>) {
		let style = style.into();
		match &style {
			CanvasStyle::Color(color) => {
				self.context.set_fill_style_str(&color.to_rgba_string());
			}
			CanvasStyle::Gradient(gradient) => {
				if let Some(browser_gradient) = self.create_browser_gradient(gradient) {
					self.context.set_fill_style_canvas_gradient(&browser_gradient);
				}
			}
			CanvasStyle::Pattern(pattern) => {
				if let Some(browser_pattern) = self.create_browser_pattern(pattern) {
					self.context.set_fill_style_canvas_pattern(&browser_pattern);
				}
			}
		}
		self.state.fill_style = style;
	}

	//==============================================================================
	/// 채울 것.
	///
	/// # 돌려줌
	/// 채울 것. (처음 검정)
	//==============================================================================
	pub fn get_fill_style(&self) -> &CanvasStyle {
		&self.state.fill_style
	}

	//==============================================================================
	/// 그을 것 설정. (원본 strokeStyle)
	///
	/// # 인자
	/// - `style`: 그을 것. (Color, "#rrggbb" 같은 CSS 색 글, CanvasGradient, CanvasPattern, 처음 검정)
	//==============================================================================
	pub fn set_stroke_style(&mut self, style: impl Into<CanvasStyle>) {
		let style = style.into();
		match &style {
			CanvasStyle::Color(color) => {
				self.context.set_stroke_style_str(&color.to_rgba_string());
			}
			CanvasStyle::Gradient(gradient) => {
				if let Some(browser_gradient) = self.create_browser_gradient(gradient) {
					self.context.set_stroke_style_canvas_gradient(&browser_gradient);
				}
			}
			CanvasStyle::Pattern(pattern) => {
				if let Some(browser_pattern) = self.create_browser_pattern(pattern) {
					self.context.set_stroke_style_canvas_pattern(&browser_pattern);
				}
			}
		}
		self.state.stroke_style = style;
	}

	//==============================================================================
	/// 그을 것.
	///
	/// # 돌려줌
	/// 그을 것. (처음 검정)
	//==============================================================================
	pub fn get_stroke_style(&self) -> &CanvasStyle {
		&self.state.stroke_style
	}

	//==============================================================================
	/// 브라우저 그라데이션 만들기. (원뿔은 web-sys 에 없어 createConicGradient 를 직접 부른다)
	//==============================================================================
	fn create_browser_gradient(&self, gradient: &CanvasGradient) -> Option<web_sys::CanvasGradient> {
		let browser_gradient = match gradient.get_kind() {
			CanvasGradientKind::Linear { start_x, start_y, end_x, end_y } => {
				self.context.create_linear_gradient(start_x as f64, start_y as f64, end_x as f64, end_y as f64)
			}
			CanvasGradientKind::Radial { start_x, start_y, start_radius, end_x, end_y, end_radius } => {
				self.context.create_radial_gradient(start_x as f64, start_y as f64, start_radius as f64, end_x as f64, end_y as f64, end_radius as f64).ok()?
			}
			CanvasGradientKind::Conic { start_angle, center_x, center_y } => {
				let function: js_sys::Function = js_sys::Reflect::get(&self.context, &JsValue::from_str("createConicGradient")).ok()?.dyn_into().ok()?;
				function.call3(&self.context, &JsValue::from_f64(start_angle as f64), &JsValue::from_f64(center_x as f64), &JsValue::from_f64(center_y as f64)).ok()?.dyn_into().ok()?
			}
		};
		for (offset, color) in gradient.get_color_stops() {
			let _ = browser_gradient.add_color_stop(*offset, &color.to_rgba_string());
		}
		Some(browser_gradient)
	}

	//==============================================================================
	/// 브라우저 패턴 만들기. (패턴의 픽셀을 떨어진 캔버스에 쓰고, 변환은 setTransform 에 DOMMatrix2DInit 객체로)
	//==============================================================================
	fn create_browser_pattern(&self, pattern: &CanvasPattern) -> Option<web_sys::CanvasPattern> {
		let pattern_canvas = create_canvas_from_pixels(pattern.width, pattern.height, &pattern.pixels)?;
		let browser_pattern = self.context.create_pattern_with_html_canvas_element(&pattern_canvas, pattern.repetition.get_name()).ok().flatten()?;
		if pattern.transform != CanvasMatrix::IDENTITY {
			let matrix_object = js_sys::Object::new();
			for (name, value) in [("a", pattern.transform.a), ("b", pattern.transform.b), ("c", pattern.transform.c), ("d", pattern.transform.d), ("e", pattern.transform.e), ("f", pattern.transform.f)] {
				let _ = js_sys::Reflect::set(&matrix_object, &JsValue::from_str(name), &JsValue::from_f64(value as f64));
			}
			if let Ok(function) = js_sys::Reflect::get(&browser_pattern, &JsValue::from_str("setTransform")).and_then(|value| value.dyn_into::<js_sys::Function>()) {
				let _ = function.call1(&browser_pattern, &matrix_object);
			}
		}
		Some(browser_pattern)
	}

	//==============================================================================
	/// 곧은 그라데이션. (원본 createLinearGradient)
	///
	/// # 인자
	/// - `start_x`: 시작 점 x.
	/// - `start_y`: 시작 점 y.
	/// - `end_x`: 끝 점 x.
	/// - `end_y`: 끝 점 y.
	///
	/// # 돌려줌
	/// 색 멈춤이 없는 곧은 그라데이션. (좌표는 칠할 때의 변환을 따른다)
	//==============================================================================
	pub fn create_linear_gradient(&self, start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> CanvasGradient {
		CanvasGradient::new(CanvasGradientKind::Linear {
			start_x,
			start_y,
			end_x,
			end_y,
		})
	}

	//==============================================================================
	/// 둥근 그라데이션. (원본 createRadialGradient, 반지름이 음수면 원본의 예외처럼 None)
	///
	/// # 인자
	/// - `start_x`: 시작 원 가운데 x.
	/// - `start_y`: 시작 원 가운데 y.
	/// - `start_radius`: 시작 원 반지름. (음수면 None)
	/// - `end_x`: 끝 원 가운데 x.
	/// - `end_y`: 끝 원 가운데 y.
	/// - `end_radius`: 끝 원 반지름. (음수면 None)
	///
	/// # 돌려줌
	/// 색 멈춤이 없는 둥근 그라데이션. 반지름이 음수면 None.
	//==============================================================================
	pub fn create_radial_gradient(&self, start_x: f32, start_y: f32, start_radius: f32, end_x: f32, end_y: f32, end_radius: f32) -> Option<CanvasGradient> {
		if start_radius < 0.0 || end_radius < 0.0 {
			return None;
		}
		Some(CanvasGradient::new(CanvasGradientKind::Radial {
			start_x,
			start_y,
			start_radius,
			end_x,
			end_y,
			end_radius,
		}))
	}

	//==============================================================================
	/// 원뿔 그라데이션. (원본 createConicGradient, 시작 각은 오른쪽에서 시계 방향 라디안)
	///
	/// # 인자
	/// - `start_angle`: 시작 각. (라디안, 0 이 오른쪽, 화면에서 시계 방향이 +)
	/// - `center_x`: 가운데 x.
	/// - `center_y`: 가운데 y.
	///
	/// # 돌려줌
	/// 색 멈춤이 없는 원뿔 그라데이션.
	//==============================================================================
	pub fn create_conic_gradient(&self, start_angle: f32, center_x: f32, center_y: f32) -> CanvasGradient {
		CanvasGradient::new(CanvasGradientKind::Conic {
			start_angle,
			center_x,
			center_y,
		})
	}

	//==============================================================================
	/// 패턴. (원본 createPattern, 그림 픽셀을 읽지 못하거나 크기가 0 이면 None)
	///
	/// # 인자
	/// - `source`: 패턴 그림. (엔진 Image, `Rc<Image>`, 다른 Canvas2D, 지금 픽셀을 떠 둔다)
	/// - `repetition`: 되풀이.
	///
	/// # 돌려줌
	/// 패턴. 그림 폭이나 높이가 0 이거나 픽셀을 읽지 못하면 None.
	//==============================================================================
	pub fn create_pattern<'a>(&self, source: impl Into<CanvasImageSource<'a>>, repetition: CanvasPatternRepetition) -> Option<CanvasPattern> {
		let source = source.into();
		let (width, height) = source.get_size();
		if width == 0 || height == 0 {
			return None;
		}
		let pixels = source.read_premultiplied_pixels()?;
		Some(CanvasPattern {
			width,
			height,
			pixels,
			repetition,
			transform: CanvasMatrix::IDENTITY,
		})
	}

	//==============================================================================
	/// 그림자 색 설정. (원본 shadowColor)
	///
	/// # 인자
	/// - `color`: 그림자 색. (Color 나 CSS 색 글, 처음 투명한 검정이라 그림자가 없다)
	//==============================================================================
	pub fn set_shadow_color(&mut self, color: impl Into<Color>) {
		self.context.set_shadow_color(&color.into().to_rgba_string());
	}

	//==============================================================================
	/// 그림자 색.
	///
	/// # 돌려줌
	/// 그림자 색. (처음 투명한 검정)
	//==============================================================================
	pub fn get_shadow_color(&self) -> Color {
		Color::from(self.context.shadow_color().as_str())
	}

	//==============================================================================
	/// 그림자 흐림 설정. (원본 shadowBlur, 음수나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `shadow_blur`: 흐림 정도. (0 이 흐리지 않음, 처음 0)
	//==============================================================================
	pub fn set_shadow_blur(&mut self, shadow_blur: f32) {
		self.context.set_shadow_blur(shadow_blur as f64);
	}

	//==============================================================================
	/// 그림자 흐림.
	///
	/// # 돌려줌
	/// 그림자 흐림. (처음 0)
	//==============================================================================
	pub fn get_shadow_blur(&self) -> f32 {
		self.context.shadow_blur() as f32
	}

	//==============================================================================
	/// 그림자 가로 밀림 설정. (원본 shadowOffsetX, 변환을 받지 않는다)
	///
	/// # 인자
	/// - `offset_x`: 가로 밀림. (캔버스 픽셀, 유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_shadow_offset_x(&mut self, offset_x: f32) {
		self.context.set_shadow_offset_x(offset_x as f64);
	}

	//==============================================================================
	/// 그림자 가로 밀림.
	///
	/// # 돌려줌
	/// 그림자 가로 밀림. (처음 0)
	//==============================================================================
	pub fn get_shadow_offset_x(&self) -> f32 {
		self.context.shadow_offset_x() as f32
	}

	//==============================================================================
	/// 그림자 세로 밀림 설정. (원본 shadowOffsetY, 변환을 받지 않는다)
	///
	/// # 인자
	/// - `offset_y`: 세로 밀림. (캔버스 픽셀, 유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_shadow_offset_y(&mut self, offset_y: f32) {
		self.context.set_shadow_offset_y(offset_y as f64);
	}

	//==============================================================================
	/// 그림자 세로 밀림.
	///
	/// # 돌려줌
	/// 그림자 세로 밀림. (처음 0)
	//==============================================================================
	pub fn get_shadow_offset_y(&self) -> f32 {
		self.context.shadow_offset_y() as f32
	}

	//==============================================================================
	/// 선 굵기 설정. (원본 lineWidth, 0 이하나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `line_width`: 선 굵기. (처음 1)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f32) {
		self.context.set_line_width(line_width as f64);
	}

	//==============================================================================
	/// 선 굵기.
	///
	/// # 돌려줌
	/// 선 굵기. (처음 1)
	//==============================================================================
	pub fn get_line_width(&self) -> f32 {
		self.context.line_width() as f32
	}

	//==============================================================================
	/// 선 끝 설정. (원본 lineCap)
	///
	/// # 인자
	/// - `line_cap`: 선 끝. (처음 Butt)
	//==============================================================================
	pub fn set_line_cap(&mut self, line_cap: CanvasLineCap) {
		let name = match line_cap {
			CanvasLineCap::Butt => {
				"butt"
			}
			CanvasLineCap::Round => {
				"round"
			}
			CanvasLineCap::Square => {
				"square"
			}
		};
		self.context.set_line_cap(name);
	}

	//==============================================================================
	/// 선 끝.
	///
	/// # 돌려줌
	/// 선 끝. (처음 Butt)
	//==============================================================================
	pub fn get_line_cap(&self) -> CanvasLineCap {
		match self.context.line_cap().as_str() {
			"round" => {
				CanvasLineCap::Round
			}
			"square" => {
				CanvasLineCap::Square
			}
			_ => {
				CanvasLineCap::Butt
			}
		}
	}

	//==============================================================================
	/// 선 이음 설정. (원본 lineJoin)
	///
	/// # 인자
	/// - `line_join`: 선 이음. (처음 Miter)
	//==============================================================================
	pub fn set_line_join(&mut self, line_join: CanvasLineJoin) {
		let name = match line_join {
			CanvasLineJoin::Miter => {
				"miter"
			}
			CanvasLineJoin::Round => {
				"round"
			}
			CanvasLineJoin::Bevel => {
				"bevel"
			}
		};
		self.context.set_line_join(name);
	}

	//==============================================================================
	/// 선 이음.
	///
	/// # 돌려줌
	/// 선 이음. (처음 Miter)
	//==============================================================================
	pub fn get_line_join(&self) -> CanvasLineJoin {
		match self.context.line_join().as_str() {
			"round" => {
				CanvasLineJoin::Round
			}
			"bevel" => {
				CanvasLineJoin::Bevel
			}
			_ => {
				CanvasLineJoin::Miter
			}
		}
	}

	//==============================================================================
	/// 뾰족 이음 한계 설정. (원본 miterLimit, 0 이하나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `miter_limit`: 뾰족 이음 한계. (이음 길이와 선 굵기의 비, 처음 10)
	//==============================================================================
	pub fn set_miter_limit(&mut self, miter_limit: f32) {
		self.context.set_miter_limit(miter_limit as f64);
	}

	//==============================================================================
	/// 뾰족 이음 한계.
	///
	/// # 돌려줌
	/// 뾰족 이음 한계. (처음 10)
	//==============================================================================
	pub fn get_miter_limit(&self) -> f32 {
		self.context.miter_limit() as f32
	}

	//==============================================================================
	/// 점선 설정. (원본 setLineDash, 음수나 유한하지 않은 값이 있으면 무시, 홀수 개면 두 번 잇는다)
	///
	/// # 인자
	/// - `dash_list`: 그을 길이와 빈 길이를 번갈아 둔 목록. (빈 목록이면 곧은 선)
	//==============================================================================
	pub fn set_line_dash(&mut self, dash_list: &[f32]) {
		if normalize_line_dash(dash_list).is_none() {
			return;
		}
		let dash_array = js_sys::Array::new();
		for dash in dash_list {
			dash_array.push(&JsValue::from_f64(*dash as f64));
		}
		let _ = self.context.set_line_dash(&dash_array);
	}

	//==============================================================================
	/// 점선. (원본 getLineDash)
	///
	/// # 돌려줌
	/// 점선 목록. (홀수 개로 넣었으면 두 번 이은 것, 처음 빈 목록)
	//==============================================================================
	pub fn get_line_dash(&self) -> Vec<f32> {
		self.context.get_line_dash().iter().filter_map(|value| value.as_f64()).map(|value| value as f32).collect()
	}

	//==============================================================================
	/// 점선 시작 밀림 설정. (원본 lineDashOffset)
	///
	/// # 인자
	/// - `line_dash_offset`: 점선을 시작할 자리. (유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_line_dash_offset(&mut self, line_dash_offset: f32) {
		self.context.set_line_dash_offset(line_dash_offset as f64);
	}

	//==============================================================================
	/// 점선 시작 밀림.
	///
	/// # 돌려줌
	/// 점선 시작 밀림. (처음 0)
	//==============================================================================
	pub fn get_line_dash_offset(&self) -> f32 {
		self.context.line_dash_offset() as f32
	}

	//==============================================================================
	/// 글꼴 설정. (원본 font)
	///
	/// # 인자
	/// - `font`: 글꼴. (CSS font 줄임 꼴, "bold 16px sans-serif" 같은 것, 처음 "10px sans-serif")
	//==============================================================================
	pub fn set_font(&mut self, font: &str) {
		self.context.set_font(font);
	}

	//==============================================================================
	/// 글꼴.
	///
	/// # 돌려줌
	/// 글꼴. (CSS font 줄임 꼴, 처음 "10px sans-serif")
	//==============================================================================
	pub fn get_font(&self) -> String {
		self.context.font()
	}

	//==============================================================================
	/// 글 가로 정렬 설정. (원본 textAlign)
	///
	/// # 인자
	/// - `text_align`: 글 가로 정렬. (처음 Start)
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: CanvasTextAlign) {
		let name = match text_align {
			CanvasTextAlign::Start => {
				"start"
			}
			CanvasTextAlign::End => {
				"end"
			}
			CanvasTextAlign::Left => {
				"left"
			}
			CanvasTextAlign::Right => {
				"right"
			}
			CanvasTextAlign::Center => {
				"center"
			}
		};
		self.context.set_text_align(name);
	}

	//==============================================================================
	/// 글 가로 정렬.
	///
	/// # 돌려줌
	/// 글 가로 정렬. (처음 Start)
	//==============================================================================
	pub fn get_text_align(&self) -> CanvasTextAlign {
		match self.context.text_align().as_str() {
			"end" => {
				CanvasTextAlign::End
			}
			"left" => {
				CanvasTextAlign::Left
			}
			"right" => {
				CanvasTextAlign::Right
			}
			"center" => {
				CanvasTextAlign::Center
			}
			_ => {
				CanvasTextAlign::Start
			}
		}
	}

	//==============================================================================
	/// 글 기준선 설정. (원본 textBaseline)
	///
	/// # 인자
	/// - `text_baseline`: 글 기준선. (처음 Alphabetic)
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: CanvasTextBaseline) {
		let name = match text_baseline {
			CanvasTextBaseline::Top => {
				"top"
			}
			CanvasTextBaseline::Hanging => {
				"hanging"
			}
			CanvasTextBaseline::Middle => {
				"middle"
			}
			CanvasTextBaseline::Alphabetic => {
				"alphabetic"
			}
			CanvasTextBaseline::Ideographic => {
				"ideographic"
			}
			CanvasTextBaseline::Bottom => {
				"bottom"
			}
		};
		self.context.set_text_baseline(name);
	}

	//==============================================================================
	/// 글 기준선.
	///
	/// # 돌려줌
	/// 글 기준선. (처음 Alphabetic)
	//==============================================================================
	pub fn get_text_baseline(&self) -> CanvasTextBaseline {
		match self.context.text_baseline().as_str() {
			"top" => {
				CanvasTextBaseline::Top
			}
			"hanging" => {
				CanvasTextBaseline::Hanging
			}
			"middle" => {
				CanvasTextBaseline::Middle
			}
			"ideographic" => {
				CanvasTextBaseline::Ideographic
			}
			"bottom" => {
				CanvasTextBaseline::Bottom
			}
			_ => {
				CanvasTextBaseline::Alphabetic
			}
		}
	}

	//==============================================================================
	/// 사각형 지우기. (원본 clearRect: 그 자리를 투명한 검정으로, 자르기는 따르고 그림자, 투명도, 합치기는 받지 않는다)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x.
	/// - `y`: 왼쪽 위 y.
	/// - `width`: 폭. (음수면 x 에서 왼쪽으로)
	/// - `height`: 높이. (음수면 y 에서 위로)
	//==============================================================================
	pub fn clear_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.context.clear_rect(x as f64, y as f64, width as f64, height as f64);
		self.revision += 1;
	}

	//==============================================================================
	/// 사각형 채우기. (원본 fillRect, 지금 경로를 바꾸지 않는다)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x.
	/// - `y`: 왼쪽 위 y.
	/// - `width`: 폭. (음수면 x 에서 왼쪽으로)
	/// - `height`: 높이. (음수면 y 에서 위로)
	//==============================================================================
	pub fn fill_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.context.fill_rect(x as f64, y as f64, width as f64, height as f64);
		self.revision += 1;
	}

	//==============================================================================
	/// 사각형 긋기. (원본 strokeRect)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x.
	/// - `y`: 왼쪽 위 y.
	/// - `width`: 폭. (음수면 x 에서 왼쪽으로)
	/// - `height`: 높이. (음수면 y 에서 위로)
	//==============================================================================
	pub fn stroke_rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.context.stroke_rect(x as f64, y as f64, width as f64, height as f64);
		self.revision += 1;
	}

	//==============================================================================
	/// 경로 비우기. (원본 beginPath)
	//==============================================================================
	pub fn begin_path(&mut self) {
		self.context.begin_path();
	}

	//==============================================================================
	/// 조각 닫기. (원본 closePath)
	//==============================================================================
	pub fn close_path(&mut self) {
		self.context.close_path();
	}

	//==============================================================================
	/// 새 조각. (원본 moveTo)
	///
	/// # 인자
	/// - `x`: 새 조각의 시작 점 x.
	/// - `y`: 새 조각의 시작 점 y.
	//==============================================================================
	pub fn move_to(&mut self, x: f32, y: f32) {
		self.context.move_to(x as f64, y as f64);
	}

	//==============================================================================
	/// 곧은 선. (원본 lineTo, 조각이 없으면 그 점에서 조각을 시작한다)
	///
	/// # 인자
	/// - `x`: 끝 점 x.
	/// - `y`: 끝 점 y.
	//==============================================================================
	pub fn line_to(&mut self, x: f32, y: f32) {
		self.context.line_to(x as f64, y as f64);
	}

	//==============================================================================
	/// 2 차 곡선. (원본 quadraticCurveTo)
	///
	/// # 인자
	/// - `control_x`: 조정점 x.
	/// - `control_y`: 조정점 y.
	/// - `x`: 끝 점 x.
	/// - `y`: 끝 점 y.
	//==============================================================================
	pub fn quadratic_curve_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
		self.context.quadratic_curve_to(control_x as f64, control_y as f64, x as f64, y as f64);
	}

	//==============================================================================
	/// 3 차 곡선. (원본 bezierCurveTo)
	///
	/// # 인자
	/// - `first_control_x`: 첫 조정점 x.
	/// - `first_control_y`: 첫 조정점 y.
	/// - `second_control_x`: 둘째 조정점 x.
	/// - `second_control_y`: 둘째 조정점 y.
	/// - `x`: 끝 점 x.
	/// - `y`: 끝 점 y.
	//==============================================================================
	pub fn bezier_curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		self.context.bezier_curve_to(first_control_x as f64, first_control_y as f64, second_control_x as f64, second_control_y as f64, x as f64, y as f64);
	}

	//==============================================================================
	/// 원호. (원본 arc, 반지름이 음수면 원본의 예외처럼 아무것도 하지 않고 false)
	///
	/// # 인자
	/// - `x`: 가운데 x.
	/// - `y`: 가운데 y.
	/// - `radius`: 반지름. (음수면 false)
	/// - `start_angle`: 시작 각. (라디안, 0 이 오른쪽, 화면에서 시계 방향이 +)
	/// - `end_angle`: 끝 각. (라디안)
	/// - `is_counterclockwise`: true 면 반시계 방향으로 돈다.
	///
	/// # 돌려줌
	/// 반지름이 음수면 false(원본의 예외 자리), 그 밖에는 true. (유한하지 않은 값이 있으면 원본처럼 아무것도 더하지 않고 true)
	//==============================================================================
	pub fn arc(&mut self, x: f32, y: f32, radius: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		self.context.arc_with_anticlockwise(x as f64, y as f64, radius as f64, start_angle as f64, end_angle as f64, is_counterclockwise).is_ok()
	}

	//==============================================================================
	/// 두 접선 사이 원호. (원본 arcTo, 반지름이 음수면 false)
	///
	/// # 인자
	/// - `first_x`: 두 접선이 만나는 점 x. (지금 점에서 이 점까지가 첫 접선)
	/// - `first_y`: 두 접선이 만나는 점 y.
	/// - `second_x`: 둘째 접선이 지나는 점 x. (만나는 점에서 이 점까지가 둘째 접선)
	/// - `second_y`: 둘째 접선이 지나는 점 y.
	/// - `radius`: 반지름. (음수면 false)
	///
	/// # 돌려줌
	/// 반지름이 음수면 false(원본의 예외 자리), 그 밖에는 true. (유한하지 않은 값이 있으면 원본처럼 아무것도 더하지 않고 true)
	//==============================================================================
	pub fn arc_to(&mut self, first_x: f32, first_y: f32, second_x: f32, second_y: f32, radius: f32) -> bool {
		self.context.arc_to(first_x as f64, first_y as f64, second_x as f64, second_y as f64, radius as f64).is_ok()
	}

	//==============================================================================
	/// 타원호. (원본 ellipse, 반지름이 음수면 false)
	///
	/// # 인자
	/// - `x`: 가운데 x.
	/// - `y`: 가운데 y.
	/// - `radius_x`: 가로 반지름. (돌리기 전, 음수면 false)
	/// - `radius_y`: 세로 반지름. (돌리기 전, 음수면 false)
	/// - `rotation`: 타원을 돌리는 각. (라디안, 화면에서 시계 방향이 +)
	/// - `start_angle`: 시작 각. (라디안, 타원 좌표계에서 0 이 오른쪽)
	/// - `end_angle`: 끝 각. (라디안)
	/// - `is_counterclockwise`: true 면 반시계 방향으로 돈다.
	///
	/// # 돌려줌
	/// 반지름이 음수면 false(원본의 예외 자리), 그 밖에는 true. (유한하지 않은 값이 있으면 원본처럼 아무것도 더하지 않고 true)
	//==============================================================================
	pub fn ellipse(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		self.context.ellipse_with_anticlockwise(x as f64, y as f64, radius_x as f64, radius_y as f64, rotation as f64, start_angle as f64, end_angle as f64, is_counterclockwise).is_ok()
	}

	//==============================================================================
	/// 사각형 조각. (원본 rect, 닫힌 조각을 더하고 (x, y) 에서 새 조각)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x.
	/// - `y`: 왼쪽 위 y.
	/// - `width`: 폭. (음수면 x 에서 왼쪽으로)
	/// - `height`: 높이. (음수면 y 에서 위로)
	//==============================================================================
	pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.context.rect(x as f64, y as f64, width as f64, height as f64);
	}

	//==============================================================================
	/// 둥근 사각형 조각. (원본 roundRect, 반지름은 수(f32)나 타원 모서리((x, y), CanvasRadius::Elliptical, 브라우저에는 DOMPointInit {x, y}))
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x.
	/// - `y`: 왼쪽 위 y.
	/// - `width`: 폭. (음수면 x 에서 왼쪽으로)
	/// - `height`: 높이. (음수면 y 에서 위로)
	/// - `radius_list`: 모서리 반지름 1 ~ 4 개. (원본처럼 1 개는 네 모서리, 2 개는 왼위와 오른아래 / 오른위와 왼아래, 3 개는 왼위 / 오른위와 왼아래 / 오른아래, 4 개는 왼위, 오른위, 오른아래, 왼아래)
	///
	/// # 돌려줌
	/// 반지름이 1 ~ 4 개가 아니거나 음수 반지름이 있으면 false(원본의 예외 자리), 그 밖에는 true.
	//==============================================================================
	pub fn round_rect<R: Into<CanvasRadius> + Copy>(&mut self, x: f32, y: f32, width: f32, height: f32, radius_list: &[R]) -> bool {
		let radius_list: Vec<CanvasRadius> = radius_list.iter().map(|radius| (*radius).into()).collect();
		self.context.round_rect_with_f64_sequence(x as f64, y as f64, width as f64, height as f64, &create_radius_array(&radius_list)).is_ok()
	}

	//==============================================================================
	/// 지금 경로 채우기. (원본 fill(fillRule))
	///
	/// # 인자
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn fill(&mut self, fill_rule: CanvasFillRule) {
		self.context.fill_with_canvas_winding_rule(convert_fill_rule(fill_rule));
		self.revision += 1;
	}

	//==============================================================================
	/// 경로 채우기. (원본 fill(path, fillRule), 경로는 지금 변환으로 옮겨 칠한다)
	///
	/// # 인자
	/// - `path`: 채울 경로.
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn fill_path(&mut self, path: &Path2D, fill_rule: CanvasFillRule) {
		if let Some(browser_path) = create_browser_path(path) {
			self.context.fill_with_path_2d_and_winding(&browser_path, convert_fill_rule(fill_rule));
			self.revision += 1;
		}
	}

	//==============================================================================
	/// 지금 경로 긋기. (원본 stroke(), 선 굵기는 지금 변환을 받는다)
	//==============================================================================
	pub fn stroke(&mut self) {
		self.context.stroke();
		self.revision += 1;
	}

	//==============================================================================
	/// 경로 긋기. (원본 stroke(path))
	///
	/// # 인자
	/// - `path`: 그을 경로. (지금 변환으로 옮겨 긋는다)
	//==============================================================================
	pub fn stroke_path(&mut self, path: &Path2D) {
		if let Some(browser_path) = create_browser_path(path) {
			self.context.stroke_with_path(&browser_path);
			self.revision += 1;
		}
	}

	//==============================================================================
	/// 지금 경로로 자르기. (원본 clip(fillRule), 이미 자른 영역과 겹친 곳만 남는다)
	///
	/// # 인자
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn clip(&mut self, fill_rule: CanvasFillRule) {
		self.context.clip_with_canvas_winding_rule(convert_fill_rule(fill_rule));
	}

	//==============================================================================
	/// 경로로 자르기. (원본 clip(path, fillRule))
	///
	/// # 인자
	/// - `path`: 자를 경로. (지금 변환으로 옮겨 자른다)
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn clip_path(&mut self, path: &Path2D, fill_rule: CanvasFillRule) {
		if let Some(browser_path) = create_browser_path(path) {
			self.context.clip_with_path_2d_and_winding(&browser_path, convert_fill_rule(fill_rule));
		}
	}

	//==============================================================================
	/// 점이 지금 경로 안에 있는지. (원본 isPointInPath(x, y, fillRule), 점은 변환을 받지 않는 캔버스 좌표)
	///
	/// # 인자
	/// - `x`: 점 x. (변환을 받지 않는 캔버스 좌표)
	/// - `y`: 점 y. (변환을 받지 않는 캔버스 좌표)
	/// - `fill_rule`: 채우기 규칙.
	///
	/// # 돌려줌
	/// 점이 안에 있으면 true.
	//==============================================================================
	pub fn is_point_in_path(&self, x: f32, y: f32, fill_rule: CanvasFillRule) -> bool {
		self.context.is_point_in_path_with_f64_and_canvas_winding_rule(x as f64, y as f64, convert_fill_rule(fill_rule))
	}

	//==============================================================================
	/// 점이 경로 안에 있는지. (원본 isPointInPath(path, x, y, fillRule), 경로는 지금 변환으로 옮겨 본다)
	///
	/// # 인자
	/// - `path`: 볼 경로.
	/// - `x`: 점 x. (변환을 받지 않는 캔버스 좌표)
	/// - `y`: 점 y. (변환을 받지 않는 캔버스 좌표)
	/// - `fill_rule`: 채우기 규칙.
	///
	/// # 돌려줌
	/// 점이 안에 있으면 true.
	//==============================================================================
	pub fn is_point_in_path2d(&self, path: &Path2D, x: f32, y: f32, fill_rule: CanvasFillRule) -> bool {
		match create_browser_path(path) {
			Some(browser_path) => {
				self.context.is_point_in_path_with_path_2d_and_f64_and_winding(&browser_path, x as f64, y as f64, convert_fill_rule(fill_rule))
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 글 재기. (원본 measureText)
	///
	/// # 인자
	/// - `text`: 잴 글.
	///
	/// # 돌려줌
	/// 잰 값. (폭, 글꼴의 위아래 높이, 실제로 칠하는 상자)
	//==============================================================================
	pub fn measure_text(&self, text: &str) -> TextMetrics {
		match self.context.measure_text(text) {
			Ok(text_metrics) => {
				TextMetrics {
					width: text_metrics.width() as f32,
					font_ascent: text_metrics.font_bounding_box_ascent() as f32,
					font_descent: text_metrics.font_bounding_box_descent() as f32,
					actual_bounding_box_left: text_metrics.actual_bounding_box_left() as f32,
					actual_bounding_box_right: text_metrics.actual_bounding_box_right() as f32,
					actual_bounding_box_ascent: text_metrics.actual_bounding_box_ascent() as f32,
					actual_bounding_box_descent: text_metrics.actual_bounding_box_descent() as f32,
				}
			}
			Err(_) => {
				TextMetrics::default()
			}
		}
	}

	//==============================================================================
	/// 글 채우기. (원본 fillText(text, x, y, maxWidth), 글이 maxWidth 보다 넓으면 가로로 좁힌다)
	///
	/// # 인자
	/// - `text`: 칠할 글.
	/// - `x`: 기준 점 x. (글 가로 정렬이 정하는 자리)
	/// - `y`: 기준 점 y. (글 기준선이 정하는 자리)
	/// - `maximum_width`: 가장 넓은 폭. (None 이면 좁히지 않는다)
	//==============================================================================
	pub fn fill_text(&mut self, text: &str, x: f32, y: f32, maximum_width: Option<f32>) {
		let _ = match maximum_width {
			Some(maximum_width) => {
				self.context.fill_text_with_max_width(text, x as f64, y as f64, maximum_width as f64)
			}
			None => {
				self.context.fill_text(text, x as f64, y as f64)
			}
		};
		self.revision += 1;
	}

	//==============================================================================
	/// 글 긋기. (원본 strokeText, 선은 lineWidth 굵기로 윤곽선 가운데에 긋는다)
	///
	/// # 인자
	/// - `text`: 그을 글.
	/// - `x`: 기준 점 x. (글 가로 정렬이 정하는 자리)
	/// - `y`: 기준 점 y. (글 기준선이 정하는 자리)
	/// - `maximum_width`: 가장 넓은 폭. (None 이면 좁히지 않는다)
	//==============================================================================
	pub fn stroke_text(&mut self, text: &str, x: f32, y: f32, maximum_width: Option<f32>) {
		let _ = match maximum_width {
			Some(maximum_width) => {
				self.context.stroke_text_with_max_width(text, x as f64, y as f64, maximum_width as f64)
			}
			None => {
				self.context.stroke_text(text, x as f64, y as f64)
			}
		};
		self.revision += 1;
	}

	//==============================================================================
	/// 그림 그리기. (원본 drawImage(image, dx, dy))
	///
	/// # 인자
	/// - `source`: 그릴 그림. (엔진 Image, `Rc<Image>`, 다른 Canvas2D)
	/// - `destination_x`: 그릴 자리 왼쪽 x.
	/// - `destination_y`: 그릴 자리 위 y.
	//==============================================================================
	pub fn draw_image<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, destination_x: f32, destination_y: f32) {
		let source = source.into();
		let (width, height) = source.get_size();
		self.draw_image_region(source, 0.0, 0.0, width as f32, height as f32, destination_x, destination_y, width as f32, height as f32);
	}

	//==============================================================================
	/// 그림 늘여 그리기. (원본 drawImage(image, dx, dy, dw, dh))
	///
	/// # 인자
	/// - `source`: 그릴 그림. (엔진 Image, `Rc<Image>`, 다른 Canvas2D)
	/// - `destination_x`: 그릴 자리 왼쪽 x.
	/// - `destination_y`: 그릴 자리 위 y.
	/// - `destination_width`: 그릴 폭. (음수면 destination_x 에서 왼쪽으로)
	/// - `destination_height`: 그릴 높이. (음수면 destination_y 에서 위로)
	//==============================================================================
	pub fn draw_image_scaled<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, destination_x: f32, destination_y: f32, destination_width: f32, destination_height: f32) {
		let source = source.into();
		let (width, height) = source.get_size();
		self.draw_image_region(source, 0.0, 0.0, width as f32, height as f32, destination_x, destination_y, destination_width, destination_height);
	}

	//==============================================================================
	/// 그림 일부 그리기. (원본 drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh). 엔진 Image 는 픽셀을 떨어진 캔버스에 써서 그린다)
	///
	/// # 인자
	/// - `source`: 그릴 그림. (엔진 Image, `Rc<Image>`, 다른 Canvas2D)
	/// - `source_x`: 떼어 올 영역 왼쪽. (그림 픽셀)
	/// - `source_y`: 떼어 올 영역 위. (그림 픽셀)
	/// - `source_width`: 떼어 올 영역 폭. (그림 픽셀, 음수면 source_x 에서 왼쪽으로)
	/// - `source_height`: 떼어 올 영역 높이. (그림 픽셀, 음수면 source_y 에서 위로)
	/// - `destination_x`: 그릴 자리 왼쪽 x.
	/// - `destination_y`: 그릴 자리 위 y.
	/// - `destination_width`: 그릴 폭. (음수면 destination_x 에서 왼쪽으로)
	/// - `destination_height`: 그릴 높이. (음수면 destination_y 에서 위로)
	//==============================================================================
	pub fn draw_image_region<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, source_x: f32, source_y: f32, source_width: f32, source_height: f32, destination_x: f32, destination_y: f32, destination_width: f32, destination_height: f32) {
		let source = source.into();
		let temporary_canvas;
		let source_canvas = match source {
			CanvasImageSource::Canvas(canvas) => {
				&canvas.canvas_element
			}
			CanvasImageSource::Image(image) => {
				let pixels = match source.read_premultiplied_pixels() {
					Some(pixels) => {
						pixels
					}
					None => {
						return;
					}
				};
				temporary_canvas = match create_canvas_from_pixels(image.get_width(), image.get_height(), &pixels) {
					Some(temporary_canvas) => {
						temporary_canvas
					}
					None => {
						return;
					}
				};
				&temporary_canvas
			}
		};
		let _ = self.context.draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(source_canvas, source_x as f64, source_y as f64, source_width as f64, source_height as f64, destination_x as f64, destination_y as f64, destination_width as f64, destination_height as f64);
		self.revision += 1;
	}

	//==============================================================================
	/// 빈 픽셀 묶음. (원본 createImageData)
	///
	/// # 인자
	/// - `width`: 폭. (픽셀)
	/// - `height`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 모든 바이트가 0 인 픽셀 묶음.
	//==============================================================================
	pub fn create_image_data(&self, width: u32, height: u32) -> ImageData {
		ImageData::new(width, height)
	}

	//==============================================================================
	/// 픽셀 읽기. (원본 getImageData, 프리멀티플라이하지 않은 RGBA. 캔버스 밖은 투명한 검정)
	///
	/// # 인자
	/// - `source_x`: 읽을 영역 왼쪽. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `source_y`: 읽을 영역 위. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `width`: 읽을 폭. (픽셀)
	/// - `height`: 읽을 높이. (픽셀)
	///
	/// # 돌려줌
	/// 프리멀티플라이하지 않은 RGBA 픽셀 묶음. (캔버스 밖은 투명한 검정)
	//==============================================================================
	pub fn get_image_data(&self, source_x: i32, source_y: i32, width: u32, height: u32) -> ImageData {
		match self.context.get_image_data(source_x as f64, source_y as f64, width as f64, height as f64) {
			Ok(browser_image_data) => {
				ImageData {
					width,
					height,
					data: browser_image_data.data().0,
				}
			}
			Err(_) => {
				ImageData::new(width, height)
			}
		}
	}

	//==============================================================================
	/// 픽셀 쓰기. (원본 putImageData: 자르기, 투명도, 합치기, 그림자 없이 그대로 바꾼다)
	///
	/// # 인자
	/// - `image_data`: 쓸 픽셀 묶음. (프리멀티플라이하지 않은 RGBA)
	/// - `destination_x`: 쓸 자리 왼쪽. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `destination_y`: 쓸 자리 위. (캔버스 픽셀, 변환을 받지 않는다)
	//==============================================================================
	pub fn put_image_data(&mut self, image_data: &ImageData, destination_x: i32, destination_y: i32) {
		if let Ok(browser_image_data) = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&image_data.data), image_data.width, image_data.height) {
			let _ = self.context.put_image_data(&browser_image_data, destination_x as f64, destination_y as f64);
			self.revision += 1;
		}
	}

	//==============================================================================
	/// 엔진 그림으로. (Graphic, UIImageView 에 쓴다. 그 뒤로 그리지 않았으면 같은 Image)
	///
	/// # 돌려줌
	/// 이 캔버스 픽셀의 엔진 그림. (그 뒤로 그리지 않았으면 같은 Rc)
	//==============================================================================
	pub fn to_image(&self) -> Rc<Image> {
		if let Some((cached_revision, cached_image)) = self.cached_image.borrow().as_ref() {
			if *cached_revision == self.revision {
				return cached_image.clone();
			}
		}
		let image = Image::create_from_premultiplied_pixels(self.width, self.height, self.read_premultiplied_pixels());
		*self.cached_image.borrow_mut() = Some((self.revision, image.clone()));
		image
	}

	//==============================================================================
	/// 프리멀티플라이드 RGBA 픽셀.
	//==============================================================================
	pub(crate) fn read_premultiplied_pixels(&self) -> Vec<u8> {
		if self.width == 0 || self.height == 0 {
			return Vec::new();
		}
		premultiply_pixels(&self.get_image_data(0, 0, self.width, self.height).data)
	}
}

//==============================================================================
/// 처음 상태. (원본 기본값)
//==============================================================================
fn create_initial_state() -> MirrorState {
	MirrorState {
		transform: CanvasMatrix::IDENTITY,
		fill_style: CanvasStyle::Color(Color::create(0.0, 0.0, 0.0, 1.0)),
		stroke_style: CanvasStyle::Color(Color::create(0.0, 0.0, 0.0, 1.0)),
	}
}

//==============================================================================
/// 채우기 규칙 → 브라우저.
//==============================================================================
fn convert_fill_rule(fill_rule: CanvasFillRule) -> CanvasWindingRule {
	match fill_rule {
		CanvasFillRule::NonZero => {
			CanvasWindingRule::Nonzero
		}
		CanvasFillRule::EvenOdd => {
			CanvasWindingRule::Evenodd
		}
	}
}

//==============================================================================
/// 둥근 사각형 반지름 → 브라우저 배열. (수는 수, 타원 모서리는 DOMPointInit {x, y})
//==============================================================================
fn create_radius_array(radius_list: &[CanvasRadius]) -> js_sys::Array {
	let radius_array = js_sys::Array::new();
	for radius in radius_list {
		match *radius {
			CanvasRadius::Circular(radius) => {
				radius_array.push(&JsValue::from_f64(radius as f64));
			}
			CanvasRadius::Elliptical {
				x,
				y,
			} => {
				let point_object = js_sys::Object::new();
				let _ = js_sys::Reflect::set(&point_object, &JsValue::from_str("x"), &JsValue::from_f64(x as f64));
				let _ = js_sys::Reflect::set(&point_object, &JsValue::from_str("y"), &JsValue::from_f64(y as f64));
				radius_array.push(&point_object);
			}
		}
	}
	radius_array
}

//==============================================================================
/// 엔진 경로 → 브라우저 Path2D. (원본처럼 브라우저 Path2D 의 함수를 같은 차례로 부른다. 원호와 둥근 사각형도 브라우저가 짓는다)
//==============================================================================
fn create_browser_path(path: &Path2D) -> Option<web_sys::Path2d> {
	let browser_path = web_sys::Path2d::new().ok()?;
	replay_browser_commands(&browser_path, path.get_browser_command_list());
	Some(browser_path)
}

//==============================================================================
/// 명령을 브라우저 Path2D 에 다시 넣기. (예외는 원본의 예외 자리라 그 명령만 건너뛴다)
//==============================================================================
fn replay_browser_commands(browser_path: &web_sys::Path2d, command_list: &[BrowserPathCommand]) {
	for command in command_list {
		match command {
			BrowserPathCommand::MoveTo(x, y) => {
				browser_path.move_to(*x as f64, *y as f64);
			}
			BrowserPathCommand::LineTo(x, y) => {
				browser_path.line_to(*x as f64, *y as f64);
			}
			BrowserPathCommand::QuadraticCurveTo(control_x, control_y, x, y) => {
				browser_path.quadratic_curve_to(*control_x as f64, *control_y as f64, *x as f64, *y as f64);
			}
			BrowserPathCommand::BezierCurveTo(first_control_x, first_control_y, second_control_x, second_control_y, x, y) => {
				browser_path.bezier_curve_to(*first_control_x as f64, *first_control_y as f64, *second_control_x as f64, *second_control_y as f64, *x as f64, *y as f64);
			}
			BrowserPathCommand::Arc(x, y, radius, start_angle, end_angle, is_counterclockwise) => {
				let _ = browser_path.arc_with_anticlockwise(*x as f64, *y as f64, *radius as f64, *start_angle as f64, *end_angle as f64, *is_counterclockwise);
			}
			BrowserPathCommand::ArcTo(first_x, first_y, second_x, second_y, radius) => {
				let _ = browser_path.arc_to(*first_x as f64, *first_y as f64, *second_x as f64, *second_y as f64, *radius as f64);
			}
			BrowserPathCommand::Ellipse(x, y, radius_x, radius_y, rotation, start_angle, end_angle, is_counterclockwise) => {
				let _ = browser_path.ellipse_with_anticlockwise(*x as f64, *y as f64, *radius_x as f64, *radius_y as f64, *rotation as f64, *start_angle as f64, *end_angle as f64, *is_counterclockwise);
			}
			BrowserPathCommand::Rect(x, y, width, height) => {
				browser_path.rect(*x as f64, *y as f64, *width as f64, *height as f64);
			}
			BrowserPathCommand::RoundRect(x, y, width, height, radius_list) => {
				let _ = browser_path.round_rect_with_f64_sequence(*x as f64, *y as f64, *width as f64, *height as f64, &create_radius_array(radius_list));
			}
			BrowserPathCommand::ClosePath => {
				browser_path.close_path();
			}
			BrowserPathCommand::AddPath(command_list, transform) => {
				let child_path = match web_sys::Path2d::new() {
					Ok(child_path) => {
						child_path
					}
					Err(_) => {
						continue;
					}
				};
				replay_browser_commands(&child_path, command_list);
				match transform {
					Some(matrix) => {
						let matrix_object = js_sys::Object::new();
						for (name, value) in [("a", matrix.a), ("b", matrix.b), ("c", matrix.c), ("d", matrix.d), ("e", matrix.e), ("f", matrix.f)] {
							let _ = js_sys::Reflect::set(&matrix_object, &JsValue::from_str(name), &JsValue::from_f64(value as f64));
						}
						let add_path_function = js_sys::Reflect::get(browser_path, &JsValue::from_str("addPath")).ok().and_then(|function_value| function_value.dyn_into::<js_sys::Function>().ok());
						if let Some(add_path_function) = add_path_function {
							let _ = add_path_function.call2(browser_path, &child_path, &matrix_object);
						}
					}
					None => {
						browser_path.add_path(&child_path);
					}
				}
			}
		}
	}
}

//==============================================================================
/// 프리멀티플라이드 RGBA 로 떨어진 캔버스 만들기. (엔진 Image 와 패턴을 브라우저에 넘길 때)
//==============================================================================
fn create_canvas_from_pixels(width: u32, height: u32, premultiplied_pixels: &[u8]) -> Option<HtmlCanvasElement> {
	let document = web_sys::window()?.document()?;
	let canvas_element: HtmlCanvasElement = document.create_element("canvas").ok()?.dyn_into().ok()?;
	canvas_element.set_width(width);
	canvas_element.set_height(height);
	let context: CanvasRenderingContext2d = canvas_element.get_context("2d").ok().flatten()?.dyn_into().ok()?;
	let straight_pixels = unpremultiply_pixels(premultiplied_pixels);
	let browser_image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(Clamped(&straight_pixels), width, height).ok()?;
	context.put_image_data(&browser_image_data, 0.0, 0.0).ok()?;
	Some(canvas_element)
}
