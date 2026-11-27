//==============================================================================
//! 2D 그리기 대상의 데스크탑 판. (tiny-skia 로 칠한다, core/canvas2d.rs 머리 주석)
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
use super::desktop_font::{parse_canvas_font, CanvasFont};
use super::{clamp_to_float, convert_conic_to_quads, saturate_to_float, evaluate_conic, is_composite_operation_name, normalize_line_dash, premultiply_pixels, unpremultiply_pixels, CanvasFillRule, CanvasGradient, CanvasGradientKind, CanvasImageSource, CanvasLineCap, CanvasLineJoin, CanvasMatrix, CanvasPattern, CanvasPatternRepetition, CanvasRadius, CanvasStyle, CanvasTextAlign, CanvasTextBaseline, ImageData, Path2D, PathSegment};
use crate::base::color::Color;
use crate::core::image::Image;
use crate::core::textshaping::ShapingDirection;
use crate::core::textstringtexturecache::{build_stroke_coverage, measure_text_without_graphic, rasterize_text_without_graphic, TextMetrics};
use std::cell::RefCell;
use std::rc::Rc;
use tiny_skia::{BlendMode, FillRule, FilterQuality, GradientStop, LineCap, LineJoin, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Point, Shader, SpreadMode, Stroke, StrokeDash, Transform};

/// 글을 굽는 배율 한계.
const MINIMUM_TEXT_SCALE: f32 = 0.25;
const MAXIMUM_TEXT_SCALE: f32 = 8.0;

//==============================================================================
/// 그리기 상태. (원본 save / restore 가 쌓는 것)
//==============================================================================
#[derive(Clone)]
struct CanvasState {
	transform: CanvasMatrix,
	fill_style: CanvasStyle,
	stroke_style: CanvasStyle,
	line_width: f32,
	line_cap: CanvasLineCap,
	line_join: CanvasLineJoin,
	miter_limit: f32,
	line_dash: Vec<f64>,
	line_dash_offset: f32,
	global_alpha: f64,
	global_composite_operation: String,
	shadow_color: Color,
	shadow_blur: f32,
	shadow_offset_x: f32,
	shadow_offset_y: f32,
	/// 글꼴. (크롬처럼 읽은 것, desktop_font.rs)
	font: CanvasFont,
	text_align: CanvasTextAlign,
	text_baseline: CanvasTextBaseline,
	is_image_smoothing_enabled: bool,
	/// 자르기. (캔버스 크기의 덮인 정도, 없으면 자르지 않는다)
	clip_mask: Option<Rc<Mask>>,
}

impl CanvasState {
	//==============================================================================
	/// 처음 상태. (원본 기본값)
	//==============================================================================
	fn new() -> CanvasState {
		CanvasState {
			transform: CanvasMatrix::IDENTITY,
			fill_style: CanvasStyle::Color(Color::create(0.0, 0.0, 0.0, 1.0)),
			stroke_style: CanvasStyle::Color(Color::create(0.0, 0.0, 0.0, 1.0)),
			line_width: 1.0,
			line_cap: CanvasLineCap::Butt,
			line_join: CanvasLineJoin::Miter,
			miter_limit: 10.0,
			line_dash: Vec::new(),
			line_dash_offset: 0.0,
			global_alpha: 1.0,
			global_composite_operation: String::from("source-over"),
			shadow_color: Color::create(0.0, 0.0, 0.0, 0.0),
			shadow_blur: 0.0,
			shadow_offset_x: 0.0,
			shadow_offset_y: 0.0,
			font: CanvasFont::create_default(),
			text_align: CanvasTextAlign::Start,
			text_baseline: CanvasTextBaseline::Alphabetic,
			is_image_smoothing_enabled: true,
			clip_mask: None,
		}
	}
}

//==============================================================================
/// 칠할 모양.
//==============================================================================
enum CanvasShape {
	/// 채운 경로. (변환은 경로 좌표 → 캔버스 좌표)
	Fill {
		path: tiny_skia::Path,
		fill_rule: FillRule,
		transform: Transform,
	},
	/// 그은 경로. (경로와 선 굵기는 그때의 사용자 좌표)
	Stroke {
		path: tiny_skia::Path,
		stroke: Stroke,
		transform: Transform,
	},
	/// 그림. (그림 좌표 → 캔버스 좌표, 자를 띠가 있으면 그 밖은 칠하지 않는다)
	Picture {
		pixmap: Pixmap,
		transform: Transform,
		quality: FilterQuality,
	},
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
	/// 그림. (폭이나 높이가 0 이면 None)
	pixmap: Option<Pixmap>,
	state: CanvasState,
	state_stack: Vec<CanvasState>,
	/// 지금 경로. (캔버스 좌표, 더할 때의 변환으로 옮겨 담는다)
	path: Path2D,
	/// 그릴 때마다 는다. (to_image 가 같은 그림을 돌려줄지 본다)
	revision: u64,
	cached_image: RefCell<Option<(u64, Rc<Image>)>>,
}

impl Canvas2D {
	//==============================================================================
	/// 만들기. (원본 document.createElement("canvas") 에 width, height 를 준 것, 투명한 검정)
	///
	/// # 인자
	/// - `width`: 폭. (픽셀)
	/// - `height`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 투명한 검정으로 채운 새 캔버스.
	//==============================================================================
	pub fn new(width: u32, height: u32) -> Canvas2D {
		Canvas2D {
			width,
			height,
			pixmap: Pixmap::new(width, height),
			state: CanvasState::new(),
			state_stack: Vec::new(),
			path: Path2D::new(),
			revision: 0,
			cached_image: RefCell::new(None),
		}
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
		self.resize(width, self.height);
	}

	//==============================================================================
	/// 높이 설정. (원본처럼 그림을 비우고 상태를 처음으로 돌린다, 같은 값이어도)
	///
	/// # 인자
	/// - `height`: 새 높이. (픽셀)
	//==============================================================================
	pub fn set_height(&mut self, height: u32) {
		self.resize(self.width, height);
	}

	//==============================================================================
	/// 크기 바꾸기. (그림과 상태를 처음으로)
	//==============================================================================
	fn resize(&mut self, width: u32, height: u32) {
		self.width = width;
		self.height = height;
		self.pixmap = Pixmap::new(width, height);
		self.reset_state();
	}

	//==============================================================================
	/// 상태를 처음으로. (상태 더미, 경로도)
	//==============================================================================
	fn reset_state(&mut self) {
		self.state = CanvasState::new();
		self.state_stack.clear();
		self.path.clear();
		self.revision += 1;
	}

	//==============================================================================
	/// 처음으로 돌리기. (원본 reset: 그림을 비우고 상태와 경로를 처음으로)
	//==============================================================================
	pub fn reset(&mut self) {
		if let Some(pixmap) = self.pixmap.as_mut() {
			pixmap.fill(tiny_skia::Color::TRANSPARENT);
		}
		self.reset_state();
	}

	//==============================================================================
	/// 상태 쌓기. (원본 save)
	//==============================================================================
	pub fn save(&mut self) {
		self.state_stack.push(self.state.clone());
	}

	//==============================================================================
	/// 상태 꺼내기. (원본 restore, 쌓은 것이 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn restore(&mut self) {
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
	pub fn scale(&mut self, x: f64, y: f64) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		self.transform(f64::from(x), 0.0, 0.0, f64::from(y), 0.0, 0.0);
	}

	//==============================================================================
	/// 돌리기. (원본 rotate, 라디안. 양수는 화면에서 시계 방향)
	///
	/// # 인자
	/// - `angle`: 돌릴 각. (라디안, 화면에서 시계 방향이 +)
	//==============================================================================
	pub fn rotate(&mut self, angle: f64) {
		let angle = clamp_to_float(angle);
		self.transform(f64::from(angle.cos()), f64::from(angle.sin()), f64::from(-angle.sin()), f64::from(angle.cos()), 0.0, 0.0);
	}

	//==============================================================================
	/// 옮기기. (원본 translate)
	///
	/// # 인자
	/// - `x`: 가로 이동. (지금 좌표계 단위, 픽셀)
	/// - `y`: 세로 이동. (지금 좌표계 단위, 픽셀)
	//==============================================================================
	pub fn translate(&mut self, x: f64, y: f64) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		self.transform(1.0, 0.0, 0.0, 1.0, f64::from(x), f64::from(y));
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
	pub fn transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
		let a = clamp_to_float(a);
		let b = clamp_to_float(b);
		let c = clamp_to_float(c);
		let d = clamp_to_float(d);
		let e = clamp_to_float(e);
		let f = clamp_to_float(f);
		let matrix = CanvasMatrix::create_float(a, b, c, d, e, f);
		if !matrix.is_finite() {
			return;
		}
		self.state.transform = self.state.transform.multiply(&matrix);
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
	pub fn set_transform(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
		let a = clamp_to_float(a);
		let b = clamp_to_float(b);
		let c = clamp_to_float(c);
		let d = clamp_to_float(d);
		let e = clamp_to_float(e);
		let f = clamp_to_float(f);
		self.set_transform_matrix(CanvasMatrix::create_float(a, b, c, d, e, f));
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
		}
	}

	//==============================================================================
	/// 변환 지우기. (원본 resetTransform)
	//==============================================================================
	pub fn reset_transform(&mut self) {
		self.state.transform = CanvasMatrix::IDENTITY;
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
	pub fn set_global_alpha(&mut self, global_alpha: f64) {
		if global_alpha.is_finite() && (0.0..=1.0).contains(&global_alpha) {
			self.state.global_alpha = global_alpha;
		}
	}

	//==============================================================================
	/// 전체 투명도.
	///
	/// # 돌려줌
	/// 전체 투명도. (0 ~ 1, 처음 1)
	//==============================================================================
	pub fn get_global_alpha(&self) -> f64 {
		self.state.global_alpha
	}

	//==============================================================================
	/// 합치기 설정. (원본 globalCompositeOperation, 모르는 이름은 무시)
	///
	/// # 인자
	/// - `operation_name`: 원본 globalCompositeOperation 이름. ("multiply" 처럼 소문자와 하이픈, 대소문자를 가린다)
	//==============================================================================
	pub fn set_global_composite_operation(&mut self, operation_name: &str) {
		if is_composite_operation_name(operation_name) {
			self.state.global_composite_operation = operation_name.to_string();
		}
	}

	//==============================================================================
	/// 합치기.
	///
	/// # 돌려줌
	/// 원본 globalCompositeOperation 이름. (처음 "source-over")
	//==============================================================================
	pub fn get_global_composite_operation(&self) -> String {
		self.state.global_composite_operation.clone()
	}

	//==============================================================================
	/// 그림 매끄럽게 설정. (원본 imageSmoothingEnabled, 끄면 가장 가까운 픽셀)
	///
	/// # 인자
	/// - `is_enabled`: true 면 매끄럽게, false 면 가장 가까운 픽셀로 그린다. (처음 true)
	//==============================================================================
	pub fn set_image_smoothing_enabled(&mut self, is_enabled: bool) {
		self.state.is_image_smoothing_enabled = is_enabled;
	}

	//==============================================================================
	/// 그림 매끄럽게 하는지.
	///
	/// # 돌려줌
	/// 매끄럽게 그리면 true. (처음 true)
	//==============================================================================
	pub fn is_image_smoothing_enabled(&self) -> bool {
		self.state.is_image_smoothing_enabled
	}

	//==============================================================================
	/// 채울 것 설정. (원본 fillStyle)
	///
	/// # 인자
	/// - `style`: 채울 것. (Color, "#rrggbb" 같은 CSS 색 글, CanvasGradient, CanvasPattern, 처음 검정)
	//==============================================================================
	pub fn set_fill_style(&mut self, style: impl Into<CanvasStyle>) {
		self.state.fill_style = style.into();
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
		self.state.stroke_style = style.into();
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
	pub fn create_linear_gradient(&self, start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> CanvasGradient {
		let start_x = clamp_to_float(start_x);
		let start_y = clamp_to_float(start_y);
		let end_x = clamp_to_float(end_x);
		let end_y = clamp_to_float(end_y);
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
	pub fn create_radial_gradient(&self, start_x: f64, start_y: f64, start_radius: f64, end_x: f64, end_y: f64, end_radius: f64) -> Option<CanvasGradient> {
		let start_x = clamp_to_float(start_x);
		let start_y = clamp_to_float(start_y);
		let start_radius = clamp_to_float(start_radius);
		let end_x = clamp_to_float(end_x);
		let end_y = clamp_to_float(end_y);
		let end_radius = clamp_to_float(end_radius);
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
	pub fn create_conic_gradient(&self, start_angle: f64, center_x: f64, center_y: f64) -> CanvasGradient {
		let start_angle = clamp_to_float(start_angle);
		let center_x = clamp_to_float(center_x);
		let center_y = clamp_to_float(center_y);
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
		self.state.shadow_color = color.into();
	}

	//==============================================================================
	/// 그림자 색.
	///
	/// # 돌려줌
	/// 그림자 색. (처음 투명한 검정)
	//==============================================================================
	pub fn get_shadow_color(&self) -> Color {
		self.state.shadow_color
	}

	//==============================================================================
	/// 그림자 흐림 설정. (원본 shadowBlur, 음수나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `shadow_blur`: 흐림 정도. (0 이 흐리지 않음, 처음 0)
	//==============================================================================
	pub fn set_shadow_blur(&mut self, shadow_blur: f64) {
		let shadow_blur = clamp_to_float(shadow_blur);
		if shadow_blur.is_finite() && shadow_blur >= 0.0 {
			self.state.shadow_blur = shadow_blur;
		}
	}

	//==============================================================================
	/// 그림자 흐림.
	///
	/// # 돌려줌
	/// 그림자 흐림. (처음 0)
	//==============================================================================
	pub fn get_shadow_blur(&self) -> f64 {
		f64::from(self.state.shadow_blur)
	}

	//==============================================================================
	/// 그림자 가로 밀림 설정. (원본 shadowOffsetX, 변환을 받지 않는다)
	///
	/// # 인자
	/// - `offset_x`: 가로 밀림. (캔버스 픽셀, 유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_shadow_offset_x(&mut self, offset_x: f64) {
		let offset_x = clamp_to_float(offset_x);
		if offset_x.is_finite() {
			self.state.shadow_offset_x = offset_x;
		}
	}

	//==============================================================================
	/// 그림자 가로 밀림.
	///
	/// # 돌려줌
	/// 그림자 가로 밀림. (처음 0)
	//==============================================================================
	pub fn get_shadow_offset_x(&self) -> f64 {
		f64::from(self.state.shadow_offset_x)
	}

	//==============================================================================
	/// 그림자 세로 밀림 설정. (원본 shadowOffsetY, 변환을 받지 않는다)
	///
	/// # 인자
	/// - `offset_y`: 세로 밀림. (캔버스 픽셀, 유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_shadow_offset_y(&mut self, offset_y: f64) {
		let offset_y = clamp_to_float(offset_y);
		if offset_y.is_finite() {
			self.state.shadow_offset_y = offset_y;
		}
	}

	//==============================================================================
	/// 그림자 세로 밀림.
	///
	/// # 돌려줌
	/// 그림자 세로 밀림. (처음 0)
	//==============================================================================
	pub fn get_shadow_offset_y(&self) -> f64 {
		f64::from(self.state.shadow_offset_y)
	}

	//==============================================================================
	/// 선 굵기 설정. (원본 lineWidth, 0 이하나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `line_width`: 선 굵기. (처음 1)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f64) {
		let line_width = clamp_to_float(line_width);
		if line_width.is_finite() && line_width > 0.0 {
			self.state.line_width = line_width;
		}
	}

	//==============================================================================
	/// 선 굵기.
	///
	/// # 돌려줌
	/// 선 굵기. (처음 1)
	//==============================================================================
	pub fn get_line_width(&self) -> f64 {
		f64::from(self.state.line_width)
	}

	//==============================================================================
	/// 선 끝 설정. (원본 lineCap)
	///
	/// # 인자
	/// - `line_cap`: 선 끝. (처음 Butt)
	//==============================================================================
	pub fn set_line_cap(&mut self, line_cap: CanvasLineCap) {
		self.state.line_cap = line_cap;
	}

	//==============================================================================
	/// 선 끝.
	///
	/// # 돌려줌
	/// 선 끝. (처음 Butt)
	//==============================================================================
	pub fn get_line_cap(&self) -> CanvasLineCap {
		self.state.line_cap
	}

	//==============================================================================
	/// 선 이음 설정. (원본 lineJoin)
	///
	/// # 인자
	/// - `line_join`: 선 이음. (처음 Miter)
	//==============================================================================
	pub fn set_line_join(&mut self, line_join: CanvasLineJoin) {
		self.state.line_join = line_join;
	}

	//==============================================================================
	/// 선 이음.
	///
	/// # 돌려줌
	/// 선 이음. (처음 Miter)
	//==============================================================================
	pub fn get_line_join(&self) -> CanvasLineJoin {
		self.state.line_join
	}

	//==============================================================================
	/// 뾰족 이음 한계 설정. (원본 miterLimit, 0 이하나 유한하지 않으면 무시)
	///
	/// # 인자
	/// - `miter_limit`: 뾰족 이음 한계. (이음 길이와 선 굵기의 비, 처음 10)
	//==============================================================================
	pub fn set_miter_limit(&mut self, miter_limit: f64) {
		let miter_limit = clamp_to_float(miter_limit);
		if miter_limit.is_finite() && miter_limit > 0.0 {
			self.state.miter_limit = miter_limit;
		}
	}

	//==============================================================================
	/// 뾰족 이음 한계.
	///
	/// # 돌려줌
	/// 뾰족 이음 한계. (처음 10)
	//==============================================================================
	pub fn get_miter_limit(&self) -> f64 {
		f64::from(self.state.miter_limit)
	}

	//==============================================================================
	/// 점선 설정. (원본 setLineDash, 음수나 유한하지 않은 값이 있으면 무시, 홀수 개면 두 번 잇는다)
	///
	/// # 인자
	/// - `dash_list`: 그을 길이와 빈 길이를 번갈아 둔 목록. (빈 목록이면 곧은 선)
	//==============================================================================
	pub fn set_line_dash(&mut self, dash_list: &[f64]) {
		if let Some(normalized_list) = normalize_line_dash(dash_list) {
			self.state.line_dash = normalized_list;
		}
	}

	//==============================================================================
	/// 점선. (원본 getLineDash)
	///
	/// # 돌려줌
	/// 점선 목록. (홀수 개로 넣었으면 두 번 이은 것, 처음 빈 목록)
	//==============================================================================
	pub fn get_line_dash(&self) -> Vec<f64> {
		self.state.line_dash.clone()
	}

	//==============================================================================
	/// 점선 시작 밀림 설정. (원본 lineDashOffset)
	///
	/// # 인자
	/// - `line_dash_offset`: 점선을 시작할 자리. (유한하지 않으면 무시, 처음 0)
	//==============================================================================
	pub fn set_line_dash_offset(&mut self, line_dash_offset: f64) {
		let line_dash_offset = clamp_to_float(line_dash_offset);
		if line_dash_offset.is_finite() {
			self.state.line_dash_offset = line_dash_offset;
		}
	}

	//==============================================================================
	/// 점선 시작 밀림.
	///
	/// # 돌려줌
	/// 점선 시작 밀림. (처음 0)
	//==============================================================================
	pub fn get_line_dash_offset(&self) -> f64 {
		f64::from(self.state.line_dash_offset)
	}

	//==============================================================================
	/// 글꼴 설정. (원본 font, CSS font 줄임 꼴. 크롬처럼 틀린 글은 무시하고, 엔진 글자 캐시가 읽는다)
	/// - 크기는 이때 픽셀로 센다. 상대 크기(em, %, larger 같은)는 문서에 붙이지 않은 캔버스처럼 10px sans-serif 에 댄다.
	/// - CSS 전역 낱말(inherit 같은), 줄높이만 있는 글, 크기나 이름이 없는 글은 틀린 글이다. 시스템 글꼴 낱말(caption 같은)은 받는다.
	///
	/// # 인자
	/// - `font`: 글꼴. (CSS font 줄임 꼴, "bold 16px sans-serif" 같은 것, 처음 "10px sans-serif")
	///
	/// # 예제
	/// 받은 글꼴은 크롬이 직렬화한 꼴로 읽히고, 틀린 글은 앞 값을 남긴다
	/// ```rust
	/// use vanilla::Canvas2D;
	///
	/// let mut canvas = Canvas2D::new(8, 8);
	/// canvas.set_font("BOLD 12pt 'Noto Sans', SANS-SERIF");
	/// assert_eq!(canvas.get_font(), "bold 16px \"Noto Sans\", sans-serif");
	/// canvas.set_font("italic 2em/1.5 serif");
	/// assert_eq!(canvas.get_font(), "italic 20px serif");
	/// canvas.set_font("inherit");
	/// assert_eq!(canvas.get_font(), "italic 20px serif");
	/// ```
	//==============================================================================
	pub fn set_font(&mut self, font: &str) {
		if font == self.state.font.source_font {
			return;
		}
		if let Some(canvas_font) = parse_canvas_font(font) {
			self.state.font = canvas_font;
		}
	}

	//==============================================================================
	/// 글꼴.
	///
	/// # 돌려줌
	/// 글꼴. (크롬 font 게터의 직렬화: "italic ", "bold " 나 굵기 수, "small-caps ", 크기 px, 이름 목록. 처음 "10px sans-serif")
	//==============================================================================
	pub fn get_font(&self) -> String {
		self.state.font.serialized_font.clone()
	}

	//==============================================================================
	/// 글 가로 정렬 설정. (원본 textAlign)
	///
	/// # 인자
	/// - `text_align`: 글 가로 정렬. (처음 Start)
	//==============================================================================
	pub fn set_text_align(&mut self, text_align: CanvasTextAlign) {
		self.state.text_align = text_align;
	}

	//==============================================================================
	/// 글 가로 정렬.
	///
	/// # 돌려줌
	/// 글 가로 정렬. (처음 Start)
	//==============================================================================
	pub fn get_text_align(&self) -> CanvasTextAlign {
		self.state.text_align
	}

	//==============================================================================
	/// 글 기준선 설정. (원본 textBaseline)
	///
	/// # 인자
	/// - `text_baseline`: 글 기준선. (처음 Alphabetic)
	//==============================================================================
	pub fn set_text_baseline(&mut self, text_baseline: CanvasTextBaseline) {
		self.state.text_baseline = text_baseline;
	}

	//==============================================================================
	/// 글 기준선.
	///
	/// # 돌려줌
	/// 글 기준선. (처음 Alphabetic)
	//==============================================================================
	pub fn get_text_baseline(&self) -> CanvasTextBaseline {
		self.state.text_baseline
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
	pub fn clear_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		let width = clamp_to_float(width);
		let height = clamp_to_float(height);
		let path = match build_rect_path(x, y, width, height) {
			Some(path) => {
				path
			}
			None => {
				return;
			}
		};
		let transform = convert_matrix(&self.state.transform);
		let clip_mask = self.state.clip_mask.clone();
		if let Some(pixmap) = self.pixmap.as_mut() {
			let mut paint = Paint::default();
			paint.set_color(tiny_skia::Color::TRANSPARENT);
			paint.blend_mode = BlendMode::Clear;
			pixmap.fill_path(&path, &paint, FillRule::Winding, transform, clip_mask.as_deref());
			self.revision += 1;
		}
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
	pub fn fill_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		let width = clamp_to_float(width);
		let height = clamp_to_float(height);
		if let Some(path) = build_rect_path(x, y, width, height) {
			let shape = CanvasShape::Fill {
				path,
				fill_rule: FillRule::Winding,
				transform: convert_matrix(&self.state.transform),
			};
			let style = self.state.fill_style.clone();
			self.draw_shape(shape, Some(&style));
		}
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
	pub fn stroke_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		let width = clamp_to_float(width);
		let height = clamp_to_float(height);
		let mut rect_path = Path2D::new();
		rect_path.rect(f64::from(x), f64::from(y), f64::from(width), f64::from(height));
		self.stroke_path(&rect_path);
	}

	//==============================================================================
	/// 경로 비우기. (원본 beginPath)
	//==============================================================================
	pub fn begin_path(&mut self) {
		self.path.clear();
	}

	//==============================================================================
	/// 조각 닫기. (원본 closePath)
	//==============================================================================
	pub fn close_path(&mut self) {
		self.path.close_path();
	}

	//==============================================================================
	/// 새 조각. (원본 moveTo)
	///
	/// # 인자
	/// - `x`: 새 조각의 시작 점 x.
	/// - `y`: 새 조각의 시작 점 y.
	//==============================================================================
	pub fn move_to(&mut self, x: f64, y: f64) {
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let matrix = self.state.transform;
		self.path.move_to_with(&matrix, x, y);
	}

	//==============================================================================
	/// 곧은 선. (원본 lineTo, 조각이 없으면 그 점에서 조각을 시작한다)
	///
	/// # 인자
	/// - `x`: 끝 점 x.
	/// - `y`: 끝 점 y.
	//==============================================================================
	pub fn line_to(&mut self, x: f64, y: f64) {
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let matrix = self.state.transform;
		self.path.line_to_with(&matrix, x, y);
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
	pub fn quadratic_curve_to(&mut self, control_x: f64, control_y: f64, x: f64, y: f64) {
		let control_x = saturate_to_float(control_x);
		let control_y = saturate_to_float(control_y);
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let matrix = self.state.transform;
		self.path.quadratic_curve_to_with(&matrix, control_x, control_y, x, y);
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
	pub fn bezier_curve_to(&mut self, first_control_x: f64, first_control_y: f64, second_control_x: f64, second_control_y: f64, x: f64, y: f64) {
		let first_control_x = saturate_to_float(first_control_x);
		let first_control_y = saturate_to_float(first_control_y);
		let second_control_x = saturate_to_float(second_control_x);
		let second_control_y = saturate_to_float(second_control_y);
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let matrix = self.state.transform;
		self.path.bezier_curve_to_with(&matrix, first_control_x, first_control_y, second_control_x, second_control_y, x, y);
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
	pub fn arc(&mut self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64, is_counterclockwise: bool) -> bool {
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let radius = saturate_to_float(radius);
		let start_angle = saturate_to_float(start_angle);
		let end_angle = saturate_to_float(end_angle);
		let matrix = self.state.transform;
		self.path.arc_with(&matrix, x, y, radius, start_angle, end_angle, is_counterclockwise)
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
	pub fn arc_to(&mut self, first_x: f64, first_y: f64, second_x: f64, second_y: f64, radius: f64) -> bool {
		let first_x = saturate_to_float(first_x);
		let first_y = saturate_to_float(first_y);
		let second_x = saturate_to_float(second_x);
		let second_y = saturate_to_float(second_y);
		let radius = saturate_to_float(radius);
		let matrix = self.state.transform;
		self.path.arc_to_with(&matrix, first_x, first_y, second_x, second_y, radius)
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
	pub fn ellipse(&mut self, x: f64, y: f64, radius_x: f64, radius_y: f64, rotation: f64, start_angle: f64, end_angle: f64, is_counterclockwise: bool) -> bool {
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let radius_x = saturate_to_float(radius_x);
		let radius_y = saturate_to_float(radius_y);
		let rotation = saturate_to_float(rotation);
		let start_angle = saturate_to_float(start_angle);
		let end_angle = saturate_to_float(end_angle);
		let matrix = self.state.transform;
		self.path.ellipse_with(&matrix, x, y, radius_x, radius_y, rotation, start_angle, end_angle, is_counterclockwise)
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
	pub fn rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		let x = saturate_to_float(x);
		let y = saturate_to_float(y);
		let width = saturate_to_float(width);
		let height = saturate_to_float(height);
		let matrix = self.state.transform;
		self.path.rect_with(&matrix, x, y, width, height);
	}

	//==============================================================================
	/// 둥근 사각형 조각. (원본 roundRect, 반지름은 수(f32)나 타원 모서리((x, y), CanvasRadius::Elliptical))
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
		let matrix = self.state.transform;
		let radius_list: Vec<CanvasRadius> = radius_list.iter().map(|radius| (*radius).into()).collect();
		self.path.round_rect_with(&matrix, x, y, width, height, &radius_list)
	}

	//==============================================================================
	/// 지금 경로 채우기. (원본 fill(fillRule))
	///
	/// # 인자
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn fill(&mut self, fill_rule: CanvasFillRule) {
		if let Some(path) = build_skia_path(&self.path, CONIC_TOLERANCE) {
			let shape = CanvasShape::Fill {
				path,
				fill_rule: convert_fill_rule(fill_rule),
				transform: Transform::identity(),
			};
			let style = self.state.fill_style.clone();
			self.draw_shape(shape, Some(&style));
		}
	}

	//==============================================================================
	/// 경로 채우기. (원본 fill(path, fillRule), 경로는 지금 변환으로 옮겨 칠한다)
	///
	/// # 인자
	/// - `path`: 채울 경로.
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn fill_path(&mut self, path: &Path2D, fill_rule: CanvasFillRule) {
		if let Some(skia_path) = build_skia_path(path, CONIC_TOLERANCE / calculate_resolution_scale(&self.state.transform)) {
			let shape = CanvasShape::Fill {
				path: skia_path,
				fill_rule: convert_fill_rule(fill_rule),
				transform: convert_matrix(&self.state.transform),
			};
			let style = self.state.fill_style.clone();
			self.draw_shape(shape, Some(&style));
		}
	}

	//==============================================================================
	/// 지금 경로 긋기. (원본 stroke(), 선 굵기는 지금 변환을 받는다)
	//==============================================================================
	pub fn stroke(&mut self) {
		let inverse_matrix = match self.state.transform.invert() {
			Some(inverse_matrix) => {
				inverse_matrix
			}
			None => {
				return;
			}
		};
		let mut user_path = Path2D::new();
		user_path.add_path(&self.path, Some(inverse_matrix));
		self.stroke_path(&user_path);
	}

	//==============================================================================
	/// 경로 긋기. (원본 stroke(path))
	///
	/// # 인자
	/// - `path`: 그을 경로. (지금 변환으로 옮겨 긋는다)
	//==============================================================================
	pub fn stroke_path(&mut self, path: &Path2D) {
		let skia_path = match build_skia_path(path, CONIC_TOLERANCE / calculate_resolution_scale(&self.state.transform)) {
			Some(skia_path) => {
				skia_path
			}
			None => {
				return;
			}
		};
		let shape = CanvasShape::Stroke {
			path: skia_path,
			stroke: self.build_stroke(),
			transform: convert_matrix(&self.state.transform),
		};
		let style = self.state.stroke_style.clone();
		self.draw_shape(shape, Some(&style));
	}

	//==============================================================================
	/// 지금 경로로 자르기. (원본 clip(fillRule), 이미 자른 영역과 겹친 곳만 남는다)
	///
	/// # 인자
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn clip(&mut self, fill_rule: CanvasFillRule) {
		let path = self.path.clone();
		self.clip_with(&path, Transform::identity(), fill_rule);
	}

	//==============================================================================
	/// 경로로 자르기. (원본 clip(path, fillRule))
	///
	/// # 인자
	/// - `path`: 자를 경로. (지금 변환으로 옮겨 자른다)
	/// - `fill_rule`: 채우기 규칙.
	//==============================================================================
	pub fn clip_path(&mut self, path: &Path2D, fill_rule: CanvasFillRule) {
		let transform = convert_matrix(&self.state.transform);
		self.clip_with(path, transform, fill_rule);
	}

	//==============================================================================
	/// 자르기. (빈 경로면 모두 잘라 낸다)
	//==============================================================================
	fn clip_with(&mut self, path: &Path2D, transform: Transform, fill_rule: CanvasFillRule) {
		let mut clip_mask = match &self.state.clip_mask {
			Some(clip_mask) => {
				clip_mask.as_ref().clone()
			}
			None => {
				match Mask::new(self.width.max(1), self.height.max(1)) {
					Some(mut clip_mask) => {
						clip_mask.data_mut().fill(255);
						clip_mask
					}
					None => {
						return;
					}
				}
			}
		};
		let resolution_scale = calculate_resolution_scale(&CanvasMatrix::create_float(transform.sx, transform.ky, transform.kx, transform.sy, transform.tx, transform.ty));
		match build_skia_path(path, CONIC_TOLERANCE / resolution_scale) {
			Some(skia_path) => {
				clip_mask.intersect_path(&skia_path, convert_fill_rule(fill_rule), true, transform);
			}
			None => {
				clip_mask.clear();
			}
		}
		self.state.clip_mask = Some(Rc::new(clip_mask));
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
	pub fn is_point_in_path(&self, x: f64, y: f64, fill_rule: CanvasFillRule) -> bool {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		is_point_inside(&self.path, &CanvasMatrix::IDENTITY, x, y, fill_rule)
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
	pub fn is_point_in_path2d(&self, path: &Path2D, x: f64, y: f64, fill_rule: CanvasFillRule) -> bool {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		is_point_inside(path, &self.state.transform, x, y, fill_rule)
	}

	//==============================================================================
	/// 글 재기. (원본 measureText, 엔진 글자 캐시로 잰다. 글꼴이 하나도 없으면 0)
	///
	/// # 인자
	/// - `text`: 잴 글.
	///
	/// # 돌려줌
	/// 잰 값. (폭, 글꼴의 위아래 높이, 실제로 칠하는 상자)
	//==============================================================================
	pub fn measure_text(&self, text: &str) -> TextMetrics {
		measure_text_without_graphic(&self.state.font.drawing_font, text).unwrap_or_default()
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
	pub fn fill_text(&mut self, text: &str, x: f64, y: f64, maximum_width: Option<f64>) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		let maximum_width = maximum_width.map(clamp_to_float);
		self.draw_text(text, x, y, maximum_width, false);
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
	pub fn stroke_text(&mut self, text: &str, x: f64, y: f64, maximum_width: Option<f64>) {
		let x = clamp_to_float(x);
		let y = clamp_to_float(y);
		let maximum_width = maximum_width.map(clamp_to_float);
		self.draw_text(text, x, y, maximum_width, true);
	}

	//==============================================================================
	/// 그림 그리기. (원본 drawImage(image, dx, dy))
	///
	/// # 인자
	/// - `source`: 그릴 그림. (엔진 Image, `Rc<Image>`, 다른 Canvas2D)
	/// - `destination_x`: 그릴 자리 왼쪽 x.
	/// - `destination_y`: 그릴 자리 위 y.
	//==============================================================================
	pub fn draw_image<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, destination_x: f64, destination_y: f64) {
		let destination_x = clamp_to_float(destination_x);
		let destination_y = clamp_to_float(destination_y);
		let source = source.into();
		let (width, height) = source.get_size();
		self.draw_image_region(source, 0.0, 0.0, width as f64, height as f64, f64::from(destination_x), f64::from(destination_y), width as f64, height as f64);
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
	pub fn draw_image_scaled<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, destination_x: f64, destination_y: f64, destination_width: f64, destination_height: f64) {
		let destination_x = clamp_to_float(destination_x);
		let destination_y = clamp_to_float(destination_y);
		let destination_width = clamp_to_float(destination_width);
		let destination_height = clamp_to_float(destination_height);
		let source = source.into();
		let (width, height) = source.get_size();
		self.draw_image_region(source, 0.0, 0.0, width as f64, height as f64, f64::from(destination_x), f64::from(destination_y), f64::from(destination_width), f64::from(destination_height));
	}

	//==============================================================================
	/// 그림 일부 그리기. (원본 drawImage(image, sx, sy, sw, sh, dx, dy, dw, dh), 음수 폭과 높이는 원본처럼 뒤집어 맞춘다)
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
	pub fn draw_image_region<'a>(&mut self, source: impl Into<CanvasImageSource<'a>>, source_x: f64, source_y: f64, source_width: f64, source_height: f64, destination_x: f64, destination_y: f64, destination_width: f64, destination_height: f64) {
		let source_x = clamp_to_float(source_x);
		let source_y = clamp_to_float(source_y);
		let source_width = clamp_to_float(source_width);
		let source_height = clamp_to_float(source_height);
		let destination_x = clamp_to_float(destination_x);
		let destination_y = clamp_to_float(destination_y);
		let destination_width = clamp_to_float(destination_width);
		let destination_height = clamp_to_float(destination_height);
		let source = source.into();
		let value_list = [source_x, source_y, source_width, source_height, destination_x, destination_y, destination_width, destination_height];
		if value_list.iter().any(|value| !value.is_finite()) || source_width == 0.0 || source_height == 0.0 || destination_width == 0.0 || destination_height == 0.0 {
			return;
		}
		let (source_x, source_width) = normalize_span(source_x, source_width);
		let (source_y, source_height) = normalize_span(source_y, source_height);
		let (destination_x, destination_width) = normalize_span(destination_x, destination_width);
		let (destination_y, destination_height) = normalize_span(destination_y, destination_height);
		let (image_width, image_height) = source.get_size();
		let pixels = match source.read_premultiplied_pixels() {
			Some(pixels) => {
				pixels
			}
			None => {
				return;
			}
		};
		// 원본 그림의 범위로 자르고, 잘린 만큼 그릴 자리도 줄인다.
		let clipped_left = source_x.max(0.0);
		let clipped_top = source_y.max(0.0);
		let clipped_right = (source_x + source_width).min(image_width as f32);
		let clipped_bottom = (source_y + source_height).min(image_height as f32);
		if clipped_right <= clipped_left || clipped_bottom <= clipped_top {
			return;
		}
		let scale_x = destination_width / source_width;
		let scale_y = destination_height / source_height;
		let crop_left = clipped_left.floor() as u32;
		let crop_top = clipped_top.floor() as u32;
		let crop_right = (clipped_right.ceil() as u32).min(image_width);
		let crop_bottom = (clipped_bottom.ceil() as u32).min(image_height);
		let crop_pixmap = match crop_pixels(&pixels, image_width, crop_left, crop_top, crop_right - crop_left, crop_bottom - crop_top) {
			Some(crop_pixmap) => {
				crop_pixmap
			}
			None => {
				return;
			}
		};
		let picture_transform = convert_matrix(&self.state.transform).pre_translate(destination_x + (crop_left as f32 - source_x) * scale_x, destination_y + (crop_top as f32 - source_y) * scale_y).pre_scale(scale_x, scale_y);
		let quality = if self.state.is_image_smoothing_enabled {
			FilterQuality::Bilinear
		}
		else {
			FilterQuality::Nearest
		};
		// 원본 그림 범위의 소수 끝을 넘지 않게 그릴 사각형으로 자른다.
		let destination_rect_path = build_rect_path(destination_x + (clipped_left - source_x) * scale_x, destination_y + (clipped_top - source_y) * scale_y, (clipped_right - clipped_left) * scale_x, (clipped_bottom - clipped_top) * scale_y);
		self.draw_picture(crop_pixmap, picture_transform, quality, destination_rect_path);
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
		let mut image_data = ImageData::new(width, height);
		let pixmap = match &self.pixmap {
			Some(pixmap) => {
				pixmap
			}
			None => {
				return image_data;
			}
		};
		let canvas_pixels = pixmap.data();
		for row_index in 0..height as i32 {
			let canvas_y = source_y + row_index;
			if canvas_y < 0 || canvas_y >= self.height as i32 {
				continue;
			}
			for column_index in 0..width as i32 {
				let canvas_x = source_x + column_index;
				if canvas_x < 0 || canvas_x >= self.width as i32 {
					continue;
				}
				let canvas_offset = (canvas_y as usize * self.width as usize + canvas_x as usize) * 4;
				let data_offset = (row_index as usize * width as usize + column_index as usize) * 4;
				image_data.data[data_offset..data_offset + 4].copy_from_slice(&canvas_pixels[canvas_offset..canvas_offset + 4]);
			}
		}
		image_data.data = unpremultiply_pixels(&image_data.data);
		image_data
	}

	//==============================================================================
	/// 픽셀 쓰기. (원본 putImageData: 자르기, 투명도, 합치기, 그림자 없이 그대로 바꾼다)
	/// - 곧은 RGBA 를 크롬처럼 (색 x 알파 + 127) / 255 로 프리멀티플라이해 담는다.
	/// - 폭이나 높이가 0 이거나 i32 를 넘거나, data 가 폭 x 높이 x 4 바이트가 아니면 쓰지 않는다. (원본 new ImageData 가 받지 않는 픽셀 묶음)
	///
	/// # 인자
	/// - `image_data`: 쓸 픽셀 묶음. (프리멀티플라이하지 않은 RGBA)
	/// - `destination_x`: 쓸 자리 왼쪽. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `destination_y`: 쓸 자리 위. (캔버스 픽셀, 변환을 받지 않는다)
	//==============================================================================
	pub fn put_image_data(&mut self, image_data: &ImageData, destination_x: i32, destination_y: i32) {
		let width = i32::try_from(image_data.width).unwrap_or(i32::MAX);
		let height = i32::try_from(image_data.height).unwrap_or(i32::MAX);
		self.put_image_data_region(image_data, destination_x, destination_y, 0, 0, width, height);
	}

	//==============================================================================
	/// 픽셀 묶음의 한 사각형만 쓰기. (원본 putImageData 의 dirtyX, dirtyY, dirtyWidth, dirtyHeight 꼴, 크롬 base_rendering_context_2d.cc 의 셈)
	/// - 폭 x 높이가 i32 를 넘으면 쓰지 않는다. 음수 폭과 높이는 시작을 그만큼 옮겨 뒤집고(시작이 음수였으면 0), 픽셀 묶음과 캔버스 밖은 자른다.
	///
	/// # 인자
	/// - `image_data`: 쓸 픽셀 묶음. (프리멀티플라이하지 않은 RGBA)
	/// - `destination_x`: 픽셀 묶음 왼쪽이 놓일 자리. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `destination_y`: 픽셀 묶음 위가 놓일 자리. (캔버스 픽셀, 변환을 받지 않는다)
	/// - `dirty_x`: 쓸 사각형 왼쪽. (픽셀 묶음 픽셀)
	/// - `dirty_y`: 쓸 사각형 위. (픽셀 묶음 픽셀)
	/// - `dirty_width`: 쓸 사각형 폭. (음수면 왼쪽으로)
	/// - `dirty_height`: 쓸 사각형 높이. (음수면 위로)
	///
	/// # 예제
	/// 2 x 2 픽셀 묶음의 오른쪽 아래 한 픽셀만 쓰기
	/// ```rust
	/// use vanilla::{Canvas2D, ImageData};
	///
	/// let mut image_data = ImageData::new(2, 2);
	/// image_data.data.copy_from_slice(&[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255]);
	///
	/// let mut canvas = Canvas2D::new(4, 4);
	/// canvas.put_image_data_region(&image_data, 1, 1, 2, 2, -1, -1);
	/// assert_eq!(canvas.get_image_data(2, 2, 1, 1).data, vec![255, 255, 255, 255]);
	/// assert_eq!(canvas.get_image_data(1, 1, 1, 1).data, vec![0, 0, 0, 0]);
	/// ```
	//==============================================================================
	pub fn put_image_data_region(&mut self, image_data: &ImageData, destination_x: i32, destination_y: i32, dirty_x: i32, dirty_y: i32, dirty_width: i32, dirty_height: i32) {
		if image_data.width == 0 || image_data.height == 0 || image_data.width > i32::MAX as u32 || image_data.height > i32::MAX as u32 {
			return;
		}
		let data_width = image_data.width as usize;
		if image_data.data.len() as u64 != image_data.width as u64 * image_data.height as u64 * 4 {
			return;
		}
		if dirty_width.checked_mul(dirty_height).is_none() {
			return;
		}
		let canvas_width = self.width as i64;
		let canvas_height = self.height as i64;
		let pixmap = match self.pixmap.as_mut() {
			Some(pixmap) => {
				pixmap
			}
			None => {
				return;
			}
		};
		// 음수 폭과 높이 뒤집기. (크롬은 절댓값을 i32 최댓값으로 포화한다)
		let normalize_span = |start: i32, length: i32| -> (i64, i64) {
			if length >= 0 {
				(start as i64, length as i64)
			}
			else if start < 0 {
				(0, 0)
			}
			else {
				(start as i64 + length as i64, (-(length as i64)).min(i32::MAX as i64))
			}
		};
		let (dirty_left, dirty_width) = normalize_span(dirty_x, dirty_width);
		let (dirty_top, dirty_height) = normalize_span(dirty_y, dirty_height);
		// 픽셀 묶음 안으로 자르고, 캔버스 자리로 옮겨 캔버스 안으로 자른다.
		let source_left = dirty_left.max(0);
		let source_top = dirty_top.max(0);
		let source_right = (dirty_left + dirty_width).min(image_data.width as i64);
		let source_bottom = (dirty_top + dirty_height).min(image_data.height as i64);
		if source_left >= source_right || source_top >= source_bottom {
			return;
		}
		let left = (source_left + destination_x as i64).max(0);
		let top = (source_top + destination_y as i64).max(0);
		let right = (source_right + destination_x as i64).min(canvas_width);
		let bottom = (source_bottom + destination_y as i64).min(canvas_height);
		if left >= right || top >= bottom {
			return;
		}
		let canvas_pixels = pixmap.data_mut();
		for canvas_y in top..bottom {
			let data_y = (canvas_y - destination_y as i64) as usize;
			let data_left = (left - destination_x as i64) as usize;
			let data_right = (right - destination_x as i64) as usize;
			let data_offset = (data_y * data_width + data_left) * 4;
			let data_end = (data_y * data_width + data_right) * 4;
			let canvas_offset = (canvas_y as usize * canvas_width as usize + left as usize) * 4;
			let premultiplied_row = premultiply_pixels(&image_data.data[data_offset..data_end]);
			canvas_pixels[canvas_offset..canvas_offset + premultiplied_row.len()].copy_from_slice(&premultiplied_row);
		}
		self.revision += 1;
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
	/// 프리멀티플라이드 RGBA 픽셀. (다른 캔버스에 그릴 때)
	//==============================================================================
	pub(crate) fn read_premultiplied_pixels(&self) -> Vec<u8> {
		match &self.pixmap {
			Some(pixmap) => {
				pixmap.data().to_vec()
			}
			None => {
				Vec::new()
			}
		}
	}

	//==============================================================================
	/// 지금 선 설정.
	//==============================================================================
	fn build_stroke(&self) -> Stroke {
		let dash = if self.state.line_dash.is_empty() {
			None
		}
		else {
			StrokeDash::new(self.state.line_dash.iter().map(|&dash| dash as f32).collect(), self.state.line_dash_offset)
		};
		Stroke {
			width: self.state.line_width,
			miter_limit: self.state.miter_limit,
			line_cap: match self.state.line_cap {
				CanvasLineCap::Butt => {
					LineCap::Butt
				}
				CanvasLineCap::Round => {
					LineCap::Round
				}
				CanvasLineCap::Square => {
					LineCap::Square
				}
			},
			line_join: match self.state.line_join {
				CanvasLineJoin::Miter => {
					LineJoin::Miter
				}
				CanvasLineJoin::Round => {
					LineJoin::Round
				}
				CanvasLineJoin::Bevel => {
					LineJoin::Bevel
				}
			},
			dash,
		}
	}

	//==============================================================================
	/// 그림자를 그리는지. (색이 보이고 흐림이나 밀림이 있을 때, 원본 규칙)
	//==============================================================================
	fn has_shadow(&self) -> bool {
		self.state.shadow_color.alpha > 0.0 && (self.state.shadow_blur > 0.0 || self.state.shadow_offset_x != 0.0 || self.state.shadow_offset_y != 0.0)
	}

	//==============================================================================
	/// 모양 칠하기. (그림자, 모양 밖도 지우는 합치기, 자르기, 전체 투명도)
	//==============================================================================
	fn draw_shape(&mut self, shape: CanvasShape, style: Option<&CanvasStyle>) {
		if self.pixmap.is_none() {
			return;
		}
		let blend_mode = convert_blend_mode(&self.state.global_composite_operation);
		let global_alpha = self.state.global_alpha as f32;
		let pattern_pixmap = style.and_then(build_pattern_pixmap);
		let clip_mask = self.build_drawing_mask(style);
		let has_shadow = self.has_shadow();
		let is_unbounded = matches!(blend_mode, BlendMode::Source | BlendMode::SourceIn | BlendMode::SourceOut | BlendMode::DestinationIn | BlendMode::DestinationAtop);
		let layer = if has_shadow || is_unbounded {
			let mut layer = match Pixmap::new(self.width, self.height) {
				Some(layer) => {
					layer
				}
				None => {
					return;
				}
			};
			let paint = build_paint(style, &self.state.transform, 1.0, BlendMode::SourceOver, pattern_pixmap.as_ref());
			render_shape(&mut layer, &shape, &paint, 1.0, BlendMode::SourceOver, None);
			Some(layer)
		}
		else {
			None
		};
		let shadow_layer = if has_shadow {
			layer.as_ref().and_then(|layer| build_shadow_layer(layer, self.state.shadow_color, self.state.shadow_blur, self.state.shadow_offset_x, self.state.shadow_offset_y))
		}
		else {
			None
		};
		let pixmap = match self.pixmap.as_mut() {
			Some(pixmap) => {
				pixmap
			}
			None => {
				return;
			}
		};
		if let Some(shadow_layer) = shadow_layer.as_ref() {
			composite_layer(pixmap, shadow_layer, global_alpha, blend_mode, clip_mask.as_deref());
		}
		match (is_unbounded, layer.as_ref()) {
			(true, Some(layer)) => {
				composite_layer(pixmap, layer, global_alpha, blend_mode, clip_mask.as_deref());
			}
			_ => {
				let paint = build_paint(style, &self.state.transform, global_alpha, blend_mode, pattern_pixmap.as_ref());
				render_shape(pixmap, &shape, &paint, global_alpha, blend_mode, clip_mask.as_deref());
			}
		}
		self.revision += 1;
	}

	//==============================================================================
	/// 칠할 영역의 덮인 정도. (자르기와, 한 방향만 되풀이하는 패턴의 띠를 겹친 것. 둘 다 없으면 None)
	//==============================================================================
	fn build_drawing_mask(&self, style: Option<&CanvasStyle>) -> Option<Rc<Mask>> {
		let pattern = match style {
			Some(CanvasStyle::Pattern(pattern)) if matches!(pattern.repetition, CanvasPatternRepetition::RepeatX | CanvasPatternRepetition::RepeatY) => {
				pattern
			}
			_ => {
				return self.state.clip_mask.clone();
			}
		};
		let mut drawing_mask = match &self.state.clip_mask {
			Some(clip_mask) => {
				clip_mask.as_ref().clone()
			}
			None => {
				let mut drawing_mask = Mask::new(self.width.max(1), self.height.max(1))?;
				drawing_mask.data_mut().fill(255);
				drawing_mask
			}
		};
		// 되풀이하지 않는 쪽은 그림 한 장 폭만 남긴다. (패턴 좌표, 칠할 때의 변환)
		let band_size = 1.0e7;
		let band_path = match pattern.repetition {
			CanvasPatternRepetition::RepeatX => {
				build_rect_path(-band_size, 0.0, band_size * 2.0, pattern.height as f32)
			}
			_ => {
				build_rect_path(0.0, -band_size, pattern.width as f32, band_size * 2.0)
			}
		}?;
		let band_transform = convert_matrix(&self.state.transform.multiply(&pattern.transform));
		drawing_mask.intersect_path(&band_path, FillRule::Winding, true, band_transform);
		Some(Rc::new(drawing_mask))
	}

	//==============================================================================
	/// 그림 칠하기. (자를 사각형이 있으면 그 밖은 칠하지 않는다)
	//==============================================================================
	fn draw_picture(&mut self, pixmap: Pixmap, transform: Transform, quality: FilterQuality, destination_rect_path: Option<tiny_skia::Path>) {
		let saved_clip_mask = self.state.clip_mask.clone();
		if let Some(destination_rect_path) = destination_rect_path {
			let mut picture_mask = match &self.state.clip_mask {
				Some(clip_mask) => {
					clip_mask.as_ref().clone()
				}
				None => {
					match Mask::new(self.width.max(1), self.height.max(1)) {
						Some(mut picture_mask) => {
							picture_mask.data_mut().fill(255);
							picture_mask
						}
						None => {
							return;
						}
					}
				}
			};
			picture_mask.intersect_path(&destination_rect_path, FillRule::Winding, true, convert_matrix(&self.state.transform));
			self.state.clip_mask = Some(Rc::new(picture_mask));
		}
		self.draw_shape(CanvasShape::Picture {
			pixmap,
			transform,
			quality,
		}, None);
		self.state.clip_mask = saved_clip_mask;
	}

	//==============================================================================
	/// 글 칠하기. (엔진 글자 캐시의 덮인 정도를 그때의 배율로 받아 칠할 것으로 채운 그림을 만들고, 색 글리프는 제 색으로 얹는다)
	//==============================================================================
	fn draw_text(&mut self, text: &str, x: f32, y: f32, maximum_width: Option<f32>, is_stroke: bool) {
		if text.is_empty() || !x.is_finite() || !y.is_finite() {
			return;
		}
		if let Some(maximum_width) = maximum_width {
			if !maximum_width.is_finite() || maximum_width <= 0.0 {
				return;
			}
		}
		let matrix = self.state.transform;
		let text_scale = ((matrix.a * matrix.a + matrix.b * matrix.b).sqrt()).max((matrix.c * matrix.c + matrix.d * matrix.d).sqrt()).clamp(MINIMUM_TEXT_SCALE, MAXIMUM_TEXT_SCALE);
		let stroke_radius = if is_stroke {
			(self.state.line_width * text_scale * 0.5).max(0.5)
		}
		else {
			0.0
		};
		let padding = stroke_radius.ceil() as i32 + 1;
		// 글자 색 밝기. (크롬 스키아 SkPaintPriv::ComputeLuminanceColor: 한 색이면 그 색, 그라데이션이나 패턴이면 회색 0.5)
		let paint_style = if is_stroke {
			&self.state.stroke_style
		}
		else {
			&self.state.fill_style
		};
		let luminance_color = match paint_style {
			CanvasStyle::Color(color) => {
				*color
			}
			_ => {
				Color::create(0.5, 0.5, 0.5, 1.0)
			}
		};
		let text_raster = match rasterize_text_without_graphic(text, &self.state.font.drawing_font, text_scale, padding, ShapingDirection::LeftToRight, luminance_color) {
			Some(text_raster) => {
				text_raster
			}
			None => {
				return;
			}
		};
		if text_raster.width <= 0 || text_raster.height <= 0 {
			return;
		}
		let coverage = if is_stroke {
			build_stroke_coverage(&text_raster.coverage, text_raster.width, text_raster.height, stroke_radius)
		}
		else {
			text_raster.coverage.clone()
		};

		// 정렬과 기준선. (Graphic 의 drawTextEntry 와 같은 셈)
		let advance_width = text_raster.advance_width as f32 / text_scale;
		let condense_scale = match maximum_width {
			Some(maximum_width) if advance_width > maximum_width => {
				maximum_width / advance_width
			}
			_ => {
				1.0
			}
		};
		let drawn_width = advance_width * condense_scale;
		let align_offset = match self.state.text_align {
			CanvasTextAlign::Center => {
				-drawn_width * 0.5
			}
			CanvasTextAlign::Right | CanvasTextAlign::End => {
				-drawn_width
			}
			CanvasTextAlign::Left | CanvasTextAlign::Start => {
				0.0
			}
		};
		let font_ascent = text_raster.font_ascent as f32 / text_scale;
		let font_descent = text_raster.font_descent as f32 / text_scale;
		let baseline_offset = match self.state.text_baseline {
			CanvasTextBaseline::Top => {
				font_ascent
			}
			CanvasTextBaseline::Hanging => {
				font_ascent * 0.8
			}
			CanvasTextBaseline::Middle => {
				(font_ascent - font_descent) * 0.5
			}
			CanvasTextBaseline::Ideographic | CanvasTextBaseline::Bottom => {
				-font_descent
			}
			CanvasTextBaseline::Alphabetic => {
				0.0
			}
		};
		let quad_x = x + align_offset - text_raster.origin_x as f32 / text_scale * condense_scale;
		let quad_y = y + baseline_offset - text_raster.baseline_y as f32 / text_scale;
		let text_matrix = matrix.multiply(&CanvasMatrix::create_float(condense_scale / text_scale, 0.0, 0.0, 1.0 / text_scale, quad_x, quad_y));

		// 글 그림. (칠할 것을 글 좌표로 채우고 덮인 정도로 깎는다)
		let width = text_raster.width as u32;
		let height = text_raster.height as u32;
		let mut text_pixmap = match Pixmap::new(width, height) {
			Some(text_pixmap) => {
				text_pixmap
			}
			None => {
				return;
			}
		};
		let style = if is_stroke {
			self.state.stroke_style.clone()
		}
		else {
			self.state.fill_style.clone()
		};
		let pattern_pixmap = build_pattern_pixmap(&style);
		let style_matrix = match text_matrix.invert() {
			Some(inverse_text_matrix) => {
				inverse_text_matrix.multiply(&matrix)
			}
			None => {
				return;
			}
		};
		let paint = build_paint(Some(&style), &style_matrix, 1.0, BlendMode::SourceOver, pattern_pixmap.as_ref());
		if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, width as f32, height as f32) {
			text_pixmap.fill_rect(rect, &paint, Transform::identity(), None);
		}
		if let Some(coverage_mask) = Mask::from_vec(coverage, tiny_skia::IntSize::from_wh(width, height).unwrap_or(tiny_skia::IntSize::from_wh(1, 1).expect("1x1"))) {
			text_pixmap.apply_mask(&coverage_mask);
		}
		// 색 글리프는 칠할 것의 색은 먹지 않고 알파만 먹는다. (원본 Canvas2D, 그은 글은 색 그대로)
		if let Some(color_pixels) = text_raster.color_pixels.as_ref() {
			let style_alpha = match &style {
				CanvasStyle::Color(color) => {
					color.alpha as f32
				}
				_ => {
					1.0
				}
			};
			if let Some(color_pixmap) = crop_pixels(color_pixels, width, 0, 0, width, height) {
				let color_paint = PixmapPaint {
					opacity: style_alpha,
					blend_mode: BlendMode::SourceOver,
					quality: FilterQuality::Nearest,
				};
				text_pixmap.draw_pixmap(0, 0, color_pixmap.as_ref(), &color_paint, Transform::identity(), None);
			}
		}
		let quality = if text_matrix.b == 0.0 && text_matrix.c == 0.0 && (text_matrix.a - 1.0).abs() < 1e-4 && (text_matrix.d - 1.0).abs() < 1e-4 {
			FilterQuality::Nearest
		}
		else {
			FilterQuality::Bilinear
		};
		self.draw_shape(CanvasShape::Picture {
			pixmap: text_pixmap,
			transform: convert_matrix(&text_matrix),
			quality,
		}, None);
	}
}

//==============================================================================
/// 행렬 → tiny-skia 변환.
//==============================================================================
fn convert_matrix(matrix: &CanvasMatrix) -> Transform {
	Transform::from_row(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f)
}

//==============================================================================
/// 채우기 규칙 → tiny-skia.
//==============================================================================
fn convert_fill_rule(fill_rule: CanvasFillRule) -> FillRule {
	match fill_rule {
		CanvasFillRule::NonZero => {
			FillRule::Winding
		}
		CanvasFillRule::EvenOdd => {
			FillRule::EvenOdd
		}
	}
}

//==============================================================================
/// 합치기 이름 → tiny-skia 섞기. (lighter 는 더하기, copy 는 바꾸기)
//==============================================================================
fn convert_blend_mode(operation_name: &str) -> BlendMode {
	match operation_name {
		"source-in" => {
			BlendMode::SourceIn
		}
		"source-out" => {
			BlendMode::SourceOut
		}
		"source-atop" => {
			BlendMode::SourceAtop
		}
		"destination-over" => {
			BlendMode::DestinationOver
		}
		"destination-in" => {
			BlendMode::DestinationIn
		}
		"destination-out" => {
			BlendMode::DestinationOut
		}
		"destination-atop" => {
			BlendMode::DestinationAtop
		}
		"lighter" => {
			BlendMode::Plus
		}
		"copy" => {
			BlendMode::Source
		}
		"xor" => {
			BlendMode::Xor
		}
		"multiply" => {
			BlendMode::Multiply
		}
		"screen" => {
			BlendMode::Screen
		}
		"overlay" => {
			BlendMode::Overlay
		}
		"darken" => {
			BlendMode::Darken
		}
		"lighten" => {
			BlendMode::Lighten
		}
		"color-dodge" => {
			BlendMode::ColorDodge
		}
		"color-burn" => {
			BlendMode::ColorBurn
		}
		"hard-light" => {
			BlendMode::HardLight
		}
		"soft-light" => {
			BlendMode::SoftLight
		}
		"difference" => {
			BlendMode::Difference
		}
		"exclusion" => {
			BlendMode::Exclusion
		}
		"hue" => {
			BlendMode::Hue
		}
		"saturation" => {
			BlendMode::Saturation
		}
		"color" => {
			BlendMode::Color
		}
		"luminosity" => {
			BlendMode::Luminosity
		}
		_ => {
			BlendMode::SourceOver
		}
	}
}

//==============================================================================
/// 엔진 색 → tiny-skia 색. (알파에 곱하기)
//==============================================================================
fn convert_color(color: Color, alpha_scale: f32) -> tiny_skia::Color {
	tiny_skia::Color::from_rgba((color.red as f32).clamp(0.0, 1.0), (color.green as f32).clamp(0.0, 1.0), (color.blue as f32).clamp(0.0, 1.0), (color.alpha as f32 * alpha_scale).clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::TRANSPARENT)
}

//==============================================================================
/// 음수 폭을 뒤집어 맞추기. (시작, 폭) → (작은 끝, 양의 폭)
//==============================================================================
fn normalize_span(start: f32, length: f32) -> (f32, f32) {
	if length < 0.0 {
		(start + length, -length)
	}
	else {
		(start, length)
	}
}

//==============================================================================
/// 사각형 경로. (폭이나 높이가 0 이어도 원본처럼 빈 모양이 된다)
//==============================================================================
fn build_rect_path(x: f32, y: f32, width: f32, height: f32) -> Option<tiny_skia::Path> {
	if ![x, y, width, height].iter().all(|value| value.is_finite()) {
		return None;
	}
	let mut path_builder = PathBuilder::new();
	path_builder.move_to(x, y);
	path_builder.line_to(x + width, y);
	path_builder.line_to(x + width, y + height);
	path_builder.line_to(x, y + height);
	path_builder.close();
	path_builder.finish()
}

//==============================================================================
/// 원뿔 곡선을 2 차 곡선으로 나누는 허용 오차. (스키아 CPU 래스터 SkEdgeBuilder 의 kConicTol, 기기 픽셀)
//==============================================================================
const CONIC_TOLERANCE: f32 = 0.25;

//==============================================================================
/// 변환의 배율. (스키아 SkMatrixPriv::ComputeResScaleForStroking: 두 열 길이 가운데 큰 쪽, 유한하지 않거나 0 이면 1)
//==============================================================================
fn calculate_resolution_scale(matrix: &CanvasMatrix) -> f32 {
	let scale_x = (matrix.a * matrix.a + matrix.b * matrix.b).sqrt();
	let scale_y = (matrix.c * matrix.c + matrix.d * matrix.d).sqrt();
	if scale_x.is_finite() && scale_y.is_finite() {
		let scale = scale_x.max(scale_y);
		if scale > 0.0 {
			return scale;
		}
	}
	1.0
}

//==============================================================================
/// 엔진 경로 → tiny-skia 경로. (비었으면 None, 원뿔 곡선은 conic_tolerance 안의 2 차 곡선들로)
//==============================================================================
fn build_skia_path(path: &Path2D, conic_tolerance: f32) -> Option<tiny_skia::Path> {
	let mut path_builder = PathBuilder::new();
	let mut last_point = (0.0, 0.0);
	let mut subpath_start_point = (0.0, 0.0);
	for segment in path.get_segment_list() {
		match *segment {
			PathSegment::MoveTo(x, y) => {
				path_builder.move_to(x, y);
				last_point = (x, y);
				subpath_start_point = (x, y);
			}
			PathSegment::LineTo(x, y) => {
				path_builder.line_to(x, y);
				last_point = (x, y);
			}
			PathSegment::QuadraticTo(control_x, control_y, x, y) => {
				path_builder.quad_to(control_x, control_y, x, y);
				last_point = (x, y);
			}
			PathSegment::CubicTo(first_control_x, first_control_y, second_control_x, second_control_y, x, y) => {
				path_builder.cubic_to(first_control_x, first_control_y, second_control_x, second_control_y, x, y);
				last_point = (x, y);
			}
			PathSegment::ConicTo(control_x, control_y, x, y, weight) => {
				let point_list = convert_conic_to_quads(last_point, (control_x, control_y), (x, y), weight, conic_tolerance);
				for quad_points in point_list[1..].chunks_exact(2) {
					path_builder.quad_to(quad_points[0].0, quad_points[0].1, quad_points[1].0, quad_points[1].1);
				}
				last_point = (x, y);
			}
			PathSegment::Close => {
				path_builder.close();
				last_point = subpath_start_point;
			}
		}
	}
	path_builder.finish()
}

//==============================================================================
/// 점이 경로 안에 있는지. (곡선은 잘게 나눈 선으로, 감김 수나 홀짝)
//==============================================================================
fn is_point_inside(path: &Path2D, matrix: &CanvasMatrix, x: f32, y: f32, fill_rule: CanvasFillRule) -> bool {
	if !x.is_finite() || !y.is_finite() {
		return false;
	}
	let mut polygon_list: Vec<Vec<(f32, f32)>> = Vec::new();
	let mut current_polygon: Vec<(f32, f32)> = Vec::new();
	let mut last_point = (0.0, 0.0);
	let curve_step_count = 16;
	for segment in path.get_segment_list() {
		match *segment {
			PathSegment::ConicTo(control_x, control_y, point_x, point_y, weight) => {
				let control = matrix.transform_point_float(control_x, control_y);
				let end = matrix.transform_point_float(point_x, point_y);
				for step_index in 1..=curve_step_count {
					let time = step_index as f32 / curve_step_count as f32;
					current_polygon.push(evaluate_conic(last_point, control, end, weight, time));
				}
				last_point = end;
			}
			PathSegment::MoveTo(point_x, point_y) => {
				if current_polygon.len() > 1 {
					polygon_list.push(std::mem::take(&mut current_polygon));
				}
				current_polygon.clear();
				last_point = matrix.transform_point_float(point_x, point_y);
				current_polygon.push(last_point);
			}
			PathSegment::LineTo(point_x, point_y) => {
				last_point = matrix.transform_point_float(point_x, point_y);
				current_polygon.push(last_point);
			}
			PathSegment::QuadraticTo(control_x, control_y, point_x, point_y) => {
				let control = matrix.transform_point_float(control_x, control_y);
				let end = matrix.transform_point_float(point_x, point_y);
				for step_index in 1..=curve_step_count {
					let time = step_index as f32 / curve_step_count as f32;
					let inverse_time = 1.0 - time;
					current_polygon.push((inverse_time * inverse_time * last_point.0 + 2.0 * inverse_time * time * control.0 + time * time * end.0, inverse_time * inverse_time * last_point.1 + 2.0 * inverse_time * time * control.1 + time * time * end.1));
				}
				last_point = end;
			}
			PathSegment::CubicTo(first_control_x, first_control_y, second_control_x, second_control_y, point_x, point_y) => {
				let first_control = matrix.transform_point_float(first_control_x, first_control_y);
				let second_control = matrix.transform_point_float(second_control_x, second_control_y);
				let end = matrix.transform_point_float(point_x, point_y);
				for step_index in 1..=curve_step_count {
					let time = step_index as f32 / curve_step_count as f32;
					let inverse_time = 1.0 - time;
					let first_weight = inverse_time * inverse_time * inverse_time;
					let second_weight = 3.0 * inverse_time * inverse_time * time;
					let third_weight = 3.0 * inverse_time * time * time;
					let fourth_weight = time * time * time;
					current_polygon.push((first_weight * last_point.0 + second_weight * first_control.0 + third_weight * second_control.0 + fourth_weight * end.0, first_weight * last_point.1 + second_weight * first_control.1 + third_weight * second_control.1 + fourth_weight * end.1));
				}
				last_point = end;
			}
			PathSegment::Close => {
				if current_polygon.len() > 1 {
					let start_point = current_polygon[0];
					polygon_list.push(std::mem::take(&mut current_polygon));
					current_polygon.push(start_point);
					last_point = start_point;
				}
			}
		}
	}
	if current_polygon.len() > 1 {
		polygon_list.push(current_polygon);
	}
	let mut winding_number = 0;
	for polygon in polygon_list.iter() {
		for point_index in 0..polygon.len() {
			let start = polygon[point_index];
			let end = polygon[(point_index + 1) % polygon.len()];
			if start.1 <= y {
				if end.1 > y && (end.0 - start.0) * (y - start.1) - (x - start.0) * (end.1 - start.1) > 0.0 {
					winding_number += 1;
				}
			}
			else if end.1 <= y && (end.0 - start.0) * (y - start.1) - (x - start.0) * (end.1 - start.1) < 0.0 {
				winding_number -= 1;
			}
		}
	}
	match fill_rule {
		CanvasFillRule::NonZero => {
			winding_number != 0
		}
		CanvasFillRule::EvenOdd => {
			winding_number % 2 != 0
		}
	}
}

//==============================================================================
/// 프리멀티플라이드 RGBA 의 일부를 그림으로. (범위가 틀리면 None)
//==============================================================================
fn crop_pixels(pixels: &[u8], image_width: u32, left: u32, top: u32, width: u32, height: u32) -> Option<Pixmap> {
	let mut pixmap = Pixmap::new(width, height)?;
	let row_length = width as usize * 4;
	for row_index in 0..height as usize {
		let source_offset = ((top as usize + row_index) * image_width as usize + left as usize) * 4;
		let source_row = pixels.get(source_offset..source_offset + row_length)?;
		pixmap.data_mut()[row_index * row_length..(row_index + 1) * row_length].copy_from_slice(source_row);
	}
	Some(pixmap)
}

//==============================================================================
/// 패턴 그림. (패턴이 아니면 None, 되풀이하지 않는 패턴은 투명한 테를 둘러 늘인 끝이 투명하게)
//==============================================================================
fn build_pattern_pixmap(style: &CanvasStyle) -> Option<Pixmap> {
	let pattern = match style {
		CanvasStyle::Pattern(pattern) => {
			pattern
		}
		_ => {
			return None;
		}
	};
	if pattern.repetition == CanvasPatternRepetition::NoRepeat {
		let mut pixmap = Pixmap::new(pattern.width + 2, pattern.height + 2)?;
		let source_pixmap = crop_pixels(&pattern.pixels, pattern.width, 0, 0, pattern.width, pattern.height)?;
		pixmap.draw_pixmap(1, 1, source_pixmap.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
		return Some(pixmap);
	}
	crop_pixels(&pattern.pixels, pattern.width, 0, 0, pattern.width, pattern.height)
}

//==============================================================================
/// 칠할 것 → tiny-skia 칠. (색, 그라데이션, 패턴. 사용자 좌표 → 칠하는 그림 좌표 행렬과 투명도를 받는다)
//==============================================================================
fn build_paint<'a>(style: Option<&CanvasStyle>, style_matrix: &CanvasMatrix, alpha: f32, blend_mode: BlendMode, pattern_pixmap: Option<&'a Pixmap>) -> Paint<'a> {
	let mut paint = Paint {
		blend_mode,
		anti_alias: true,
		..Default::default()
	};
	let style = match style {
		Some(style) => {
			style
		}
		None => {
			return paint;
		}
	};
	let style_transform = convert_matrix(style_matrix);
	paint.shader = match style {
		CanvasStyle::Color(color) => {
			Shader::SolidColor(convert_color(*color, alpha))
		}
		CanvasStyle::Gradient(gradient) => {
			build_gradient_shader(gradient, style_transform, alpha)
		}
		CanvasStyle::Pattern(pattern) => {
			match pattern_pixmap {
				Some(pattern_pixmap) => {
					let (spread_mode, pattern_offset) = if pattern.repetition == CanvasPatternRepetition::NoRepeat {
						(SpreadMode::Pad, -1.0)
					}
					else {
						(SpreadMode::Repeat, 0.0)
					};
					let pattern_transform = style_transform.pre_concat(convert_matrix(&pattern.transform)).pre_translate(pattern_offset, pattern_offset);
					tiny_skia::Pattern::new(pattern_pixmap.as_ref(), spread_mode, FilterQuality::Bilinear, alpha, pattern_transform)
				}
				None => {
					Shader::SolidColor(tiny_skia::Color::TRANSPARENT)
				}
			}
		}
	};
	paint
}

//==============================================================================
/// 그라데이션 칠. (멈춤이 없으면 투명, 하나면 그 색. 원본처럼 같은 자리 멈춤은 날카롭게 바뀐다)
//==============================================================================
fn build_gradient_shader(gradient: &CanvasGradient, style_transform: Transform, alpha: f32) -> Shader<'static> {
	let stop_list: Vec<GradientStop> = gradient.get_color_stops().iter().map(|(offset, color)| GradientStop::new(*offset, convert_color(*color, alpha))).collect();
	if stop_list.is_empty() {
		return Shader::SolidColor(tiny_skia::Color::TRANSPARENT);
	}
	let shader = match gradient.get_kind() {
		CanvasGradientKind::Linear { start_x, start_y, end_x, end_y } => {
			tiny_skia::LinearGradient::new(Point::from_xy(start_x, start_y), Point::from_xy(end_x, end_y), stop_list, SpreadMode::Pad, style_transform)
		}
		CanvasGradientKind::Radial { start_x, start_y, start_radius, end_x, end_y, end_radius } => {
			tiny_skia::RadialGradient::new(Point::from_xy(start_x, start_y), start_radius, Point::from_xy(end_x, end_y), end_radius, stop_list, SpreadMode::Pad, style_transform)
		}
		CanvasGradientKind::Conic { start_angle, center_x, center_y } => {
			let conic_transform = style_transform.pre_translate(center_x, center_y).pre_concat(Transform::from_rotate(start_angle.to_degrees()));
			tiny_skia::SweepGradient::new(Point::from_xy(0.0, 0.0), 0.0, 360.0, stop_list, SpreadMode::Pad, conic_transform)
		}
	};
	// 원본은 시작과 끝이 같은 곧은 그라데이션 같은 것을 아무것도 칠하지 않는다.
	shader.unwrap_or(Shader::SolidColor(tiny_skia::Color::TRANSPARENT))
}

//==============================================================================
/// 모양 하나 칠하기.
//==============================================================================
fn render_shape(target: &mut Pixmap, shape: &CanvasShape, paint: &Paint, alpha: f32, blend_mode: BlendMode, mask: Option<&Mask>) {
	match shape {
		CanvasShape::Fill { path, fill_rule, transform } => {
			target.fill_path(path, paint, *fill_rule, *transform, mask);
		}
		CanvasShape::Stroke { path, stroke, transform } => {
			target.stroke_path(path, paint, stroke, *transform, mask);
		}
		CanvasShape::Picture { pixmap, transform, quality } => {
			let pixmap_paint = PixmapPaint {
				opacity: alpha,
				blend_mode,
				quality: *quality,
			};
			target.draw_pixmap(0, 0, pixmap.as_ref(), &pixmap_paint, *transform, mask);
		}
	}
}

//==============================================================================
/// 따로 칠한 층을 캔버스 전체에 합치기. (모양 밖의 투명한 곳도 합치기를 받는다)
//==============================================================================
fn composite_layer(target: &mut Pixmap, layer: &Pixmap, alpha: f32, blend_mode: BlendMode, mask: Option<&Mask>) {
	let pixmap_paint = PixmapPaint {
		opacity: alpha,
		blend_mode,
		quality: FilterQuality::Nearest,
	};
	target.draw_pixmap(0, 0, layer.as_ref(), &pixmap_paint, Transform::identity(), mask);
}

//==============================================================================
/// 그림자 층. (모양의 알파를 흐리고 그림자 색으로 칠해 밀림만큼 옮긴다. 흐림 시그마는 shadowBlur / 2)
//==============================================================================
fn build_shadow_layer(layer: &Pixmap, shadow_color: Color, shadow_blur: f32, offset_x: f32, offset_y: f32) -> Option<Pixmap> {
	let width = layer.width() as usize;
	let height = layer.height() as usize;
	let mut alpha_plane: Vec<f32> = layer.data().chunks_exact(4).map(|pixel| pixel[3] as f32).collect();
	let sigma = shadow_blur * 0.5;
	if sigma > 0.0 {
		blur_alpha_plane(&mut alpha_plane, width, height, sigma);
	}
	let mut shadow_layer = Pixmap::new(layer.width(), layer.height())?;
	let shadow_alpha = (shadow_color.alpha as f32).clamp(0.0, 1.0);
	let red = (shadow_color.red as f32).clamp(0.0, 1.0);
	let green = (shadow_color.green as f32).clamp(0.0, 1.0);
	let blue = (shadow_color.blue as f32).clamp(0.0, 1.0);
	let shift_x = offset_x.round() as i64;
	let shift_y = offset_y.round() as i64;
	let shadow_pixels = shadow_layer.data_mut();
	for y in 0..height as i64 {
		let source_y = y - shift_y;
		if source_y < 0 || source_y >= height as i64 {
			continue;
		}
		for x in 0..width as i64 {
			let source_x = x - shift_x;
			if source_x < 0 || source_x >= width as i64 {
				continue;
			}
			let alpha = alpha_plane[source_y as usize * width + source_x as usize] / 255.0 * shadow_alpha;
			if alpha <= 0.0 {
				continue;
			}
			let offset = (y as usize * width + x as usize) * 4;
			shadow_pixels[offset] = (red * alpha * 255.0).round() as u8;
			shadow_pixels[offset + 1] = (green * alpha * 255.0).round() as u8;
			shadow_pixels[offset + 2] = (blue * alpha * 255.0).round() as u8;
			shadow_pixels[offset + 3] = (alpha * 255.0).round() as u8;
		}
	}
	Some(shadow_layer)
}

//==============================================================================
/// 알파 흐리기. (가우스를 상자 흐림 세 번으로, 가로 다음 세로)
//==============================================================================
fn blur_alpha_plane(alpha_plane: &mut [f32], width: usize, height: usize, sigma: f32) {
	let box_size_list = compute_box_sizes(sigma, 3);
	let mut scratch = vec![0.0f32; alpha_plane.len()];
	for box_size in box_size_list {
		let radius = (box_size - 1) / 2;
		box_blur_horizontal(alpha_plane, &mut scratch, width, height, radius);
		box_blur_vertical(&scratch, alpha_plane, width, height, radius);
	}
}

//==============================================================================
/// 가우스를 닮은 상자 크기들. (홀수, 셋을 이어 시그마가 되게)
//==============================================================================
fn compute_box_sizes(sigma: f32, box_count: usize) -> Vec<usize> {
	let ideal_width = ((12.0 * sigma * sigma / box_count as f32) + 1.0).sqrt();
	let mut lower_width = ideal_width.floor() as usize;
	if lower_width % 2 == 0 {
		lower_width = lower_width.saturating_sub(1);
	}
	lower_width = lower_width.max(1);
	let upper_width = lower_width + 2;
	let ideal_lower_count = (12.0 * sigma * sigma - (box_count * lower_width * lower_width) as f32 - (4 * box_count * lower_width) as f32 - (3 * box_count) as f32) / (-4.0 * lower_width as f32 - 4.0);
	let lower_count = ideal_lower_count.round().clamp(0.0, box_count as f32) as usize;
	(0..box_count).map(|box_index| if box_index < lower_count { lower_width } else { upper_width }).collect()
}

//==============================================================================
/// 가로 상자 흐림. (밖은 0)
//==============================================================================
fn box_blur_horizontal(source: &[f32], target: &mut [f32], width: usize, height: usize, radius: usize) {
	let window_size = (radius * 2 + 1) as f32;
	for y in 0..height {
		let row = &source[y * width..(y + 1) * width];
		let mut sum: f32 = row.iter().take(radius.min(width)).sum();
		for x in 0..width {
			if x + radius < width {
				sum += row[x + radius];
			}
			if x > radius {
				sum -= row[x - radius - 1];
			}
			target[y * width + x] = sum / window_size;
		}
	}
}

//==============================================================================
/// 세로 상자 흐림. (밖은 0)
//==============================================================================
fn box_blur_vertical(source: &[f32], target: &mut [f32], width: usize, height: usize, radius: usize) {
	let window_size = (radius * 2 + 1) as f32;
	for x in 0..width {
		let mut sum: f32 = (0..radius.min(height)).map(|y| source[y * width + x]).sum();
		for y in 0..height {
			if y + radius < height {
				sum += source[(y + radius) * width + x];
			}
			if y > radius {
				sum -= source[(y - radius - 1) * width + x];
			}
			target[y * width + x] = sum / window_size;
		}
	}
}

//==============================================================================
/// 시험. (픽셀은 크롬 Canvas2D 와 같은 꼴로 나오는지 본다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 한 픽셀의 곧은 RGBA.
	//==============================================================================
	fn read_pixel(canvas: &Canvas2D, x: i32, y: i32) -> [u8; 4] {
		let image_data = canvas.get_image_data(x, y, 1, 1);
		[image_data.data[0], image_data.data[1], image_data.data[2], image_data.data[3]]
	}

	//==============================================================================
	/// 사각형, 투명도, 지우기, 변환, 픽셀 읽고 쓰기, 엔진 그림.
	//==============================================================================
	#[test]
	fn rect_alpha_transform_and_pixels() {
		let mut canvas = Canvas2D::new(20, 10);
		canvas.set_fill_style("#ff0000");
		canvas.fill_rect(0.0, 0.0, 10.0, 10.0);
		assert_eq!(read_pixel(&canvas, 5, 5), [255, 0, 0, 255]);
		assert_eq!(read_pixel(&canvas, 15, 5), [0, 0, 0, 0]);
		canvas.set_global_alpha(0.5);
		canvas.set_fill_style(Color::create(0.0, 0.0, 1.0, 1.0));
		canvas.fill_rect(10.0, 0.0, 10.0, 10.0);
		assert_eq!(read_pixel(&canvas, 15, 5), [0, 0, 255, 128]);
		canvas.set_global_alpha(2.0);
		assert_eq!(canvas.get_global_alpha(), 0.5);
		canvas.set_global_alpha(1.0);
		canvas.clear_rect(0.0, 0.0, 5.0, 10.0);
		assert_eq!(read_pixel(&canvas, 2, 5), [0, 0, 0, 0]);
		assert_eq!(read_pixel(&canvas, 7, 5), [255, 0, 0, 255]);
		canvas.save();
		canvas.translate(10.0, 0.0);
		canvas.scale(0.5, 1.0);
		canvas.set_fill_style("#00ff00");
		canvas.fill_rect(0.0, 0.0, 4.0, 2.0);
		assert_eq!(canvas.get_transform(), CanvasMatrix::create(0.5, 0.0, 0.0, 1.0, 10.0, 0.0));
		canvas.restore();
		assert_eq!(canvas.get_transform(), CanvasMatrix::IDENTITY);
		assert_eq!(read_pixel(&canvas, 11, 1), [0, 255, 0, 255]);
		assert_eq!(read_pixel(&canvas, 12, 1), [0, 0, 255, 128]);

		let mut image_data = canvas.create_image_data(2, 1);
		image_data.data.copy_from_slice(&[10, 20, 30, 255, 200, 100, 50, 128]);
		canvas.put_image_data(&image_data, 0, 9);
		// 반투명 픽셀은 원본(크롬)처럼 프리멀티플라이드로 들어 되읽으면 끝자리가 바뀔 수 있다. (200 → 100 → 199)
		let read_data = canvas.get_image_data(0, 9, 2, 1).data;
		assert_eq!(&read_data[0..4], &[10, 20, 30, 255]);
		assert!((read_data[4] as i32 - 200).abs() <= 1 && read_data[5] == 100 && read_data[6] == 50 && read_data[7] == 128);
		let first_image = canvas.to_image();
		assert!(Rc::ptr_eq(&first_image, &canvas.to_image()));
		assert_eq!((first_image.get_width(), first_image.get_height()), (20, 10));
		canvas.fill_rect(0.0, 0.0, 1.0, 1.0);
		assert!(!Rc::ptr_eq(&first_image, &canvas.to_image()));
		canvas.set_width(4);
		assert_eq!(read_pixel(&canvas, 1, 1), [0, 0, 0, 0]);
		assert_eq!(canvas.get_fill_style(), &CanvasStyle::Color(Color::create(0.0, 0.0, 0.0, 1.0)));
	}

	//==============================================================================
	/// 경로: 원, 짝홀 채우기, 점 안에 있는지, 자르기, 긋기와 점선.
	//==============================================================================
	#[test]
	fn path_fill_clip_and_stroke() {
		let mut canvas = Canvas2D::new(40, 40);
		canvas.begin_path();
		assert!(canvas.arc(20.0, 20.0, 10.0, 0.0, 2.0 * std::f64::consts::PI, false));
		assert!(!canvas.arc(20.0, 20.0, -1.0, 0.0, 1.0, false));
		assert!(canvas.is_point_in_path(20.0, 20.0, CanvasFillRule::NonZero));
		assert!(!canvas.is_point_in_path(2.0, 2.0, CanvasFillRule::NonZero));
		canvas.set_fill_style("#000000");
		canvas.fill(CanvasFillRule::NonZero);
		assert_eq!(read_pixel(&canvas, 20, 20)[3], 255);
		assert_eq!(read_pixel(&canvas, 20, 9)[3], 0);
		assert_eq!(read_pixel(&canvas, 3, 3)[3], 0);

		// 짝홀: 큰 사각형 안의 작은 사각형은 구멍.
		let mut ring = Path2D::new();
		ring.rect(0.0, 0.0, 40.0, 40.0);
		ring.rect(10.0, 10.0, 20.0, 20.0);
		let mut ring_canvas = Canvas2D::new(40, 40);
		ring_canvas.fill_path(&ring, CanvasFillRule::EvenOdd);
		assert_eq!(read_pixel(&ring_canvas, 5, 5)[3], 255);
		assert_eq!(read_pixel(&ring_canvas, 20, 20)[3], 0);
		assert!(!ring_canvas.is_point_in_path2d(&ring, 20.0, 20.0, CanvasFillRule::EvenOdd));
		assert!(ring_canvas.is_point_in_path2d(&ring, 20.0, 20.0, CanvasFillRule::NonZero));

		// 자르기: 왼쪽 절반만 칠해진다. restore 로 풀린다.
		let mut clip_canvas = Canvas2D::new(20, 10);
		clip_canvas.save();
		clip_canvas.rect(0.0, 0.0, 10.0, 10.0);
		clip_canvas.clip(CanvasFillRule::NonZero);
		clip_canvas.fill_rect(0.0, 0.0, 20.0, 10.0);
		assert_eq!(read_pixel(&clip_canvas, 5, 5)[3], 255);
		assert_eq!(read_pixel(&clip_canvas, 15, 5)[3], 0);
		clip_canvas.restore();
		clip_canvas.fill_rect(15.0, 0.0, 5.0, 10.0);
		assert_eq!(read_pixel(&clip_canvas, 17, 5)[3], 255);

		// 긋기: 굵기 4 의 가로선, 점선은 빈 곳이 있다.
		let mut stroke_canvas = Canvas2D::new(40, 10);
		stroke_canvas.set_line_width(4.0);
		stroke_canvas.set_stroke_style("#0000ff");
		stroke_canvas.set_line_dash(&[5.0]);
		assert_eq!(stroke_canvas.get_line_dash(), vec![5.0, 5.0]);
		stroke_canvas.set_line_dash(&[-1.0]);
		assert_eq!(stroke_canvas.get_line_dash(), vec![5.0, 5.0]);
		stroke_canvas.begin_path();
		stroke_canvas.move_to(0.0, 5.0);
		stroke_canvas.line_to(40.0, 5.0);
		stroke_canvas.stroke();
		assert_eq!(read_pixel(&stroke_canvas, 2, 5), [0, 0, 255, 255]);
		assert_eq!(read_pixel(&stroke_canvas, 7, 5)[3], 0);
		assert_eq!(read_pixel(&stroke_canvas, 2, 1)[3], 0);
	}

	//==============================================================================
	/// 그라데이션, 그림자, 모양 밖도 지우는 합치기, 그림 그리기와 패턴.
	//==============================================================================
	#[test]
	fn gradient_shadow_composite_and_image() {
		let mut canvas = Canvas2D::new(100, 10);
		let mut gradient = canvas.create_linear_gradient(0.0, 0.0, 100.0, 0.0);
		assert!(gradient.add_color_stop(0.0, "#000000"));
		assert!(gradient.add_color_stop(1.0, "#ffffff"));
		assert!(!gradient.add_color_stop(1.5, "#ff0000"));
		canvas.set_fill_style(&gradient);
		canvas.fill_rect(0.0, 0.0, 100.0, 10.0);
		let left_pixel = read_pixel(&canvas, 1, 5);
		let middle_pixel = read_pixel(&canvas, 50, 5);
		let right_pixel = read_pixel(&canvas, 98, 5);
		assert!(left_pixel[0] < 10 && right_pixel[0] > 245);
		assert!((middle_pixel[0] as i32 - 128).abs() <= 3);
		assert!(canvas.create_radial_gradient(0.0, 0.0, -1.0, 0.0, 0.0, 1.0).is_none());

		// 그림자: 모양 밖 오른쪽 아래로 흐린 그림자가 깔린다.
		let mut shadow_canvas = Canvas2D::new(60, 60);
		shadow_canvas.set_shadow_color("rgba(0, 0, 0, 1)");
		shadow_canvas.set_shadow_blur(8.0);
		shadow_canvas.set_shadow_offset_x(10.0);
		shadow_canvas.set_shadow_offset_y(10.0);
		shadow_canvas.set_fill_style("#ff0000");
		shadow_canvas.fill_rect(10.0, 10.0, 20.0, 20.0);
		assert_eq!(read_pixel(&shadow_canvas, 20, 20), [255, 0, 0, 255]);
		let shadow_pixel = read_pixel(&shadow_canvas, 35, 35);
		assert_eq!(&shadow_pixel[0..3], &[0, 0, 0]);
		assert!(shadow_pixel[3] > 100);
		let edge_pixel = read_pixel(&shadow_canvas, 42, 25);
		assert!(edge_pixel[3] > 0 && edge_pixel[3] < 200);
		assert_eq!(read_pixel(&shadow_canvas, 2, 2)[3], 0);

		// copy 는 모양 밖을 지운다. source-over 로 칠한 왼쪽 위가 사라진다.
		let mut copy_canvas = Canvas2D::new(20, 20);
		copy_canvas.fill_rect(0.0, 0.0, 20.0, 20.0);
		copy_canvas.set_global_composite_operation("copy");
		copy_canvas.set_global_composite_operation("unknown");
		assert_eq!(copy_canvas.get_global_composite_operation(), "copy");
		copy_canvas.set_fill_style("#00ff00");
		copy_canvas.fill_rect(10.0, 10.0, 10.0, 10.0);
		assert_eq!(read_pixel(&copy_canvas, 5, 5)[3], 0);
		assert_eq!(read_pixel(&copy_canvas, 15, 15), [0, 255, 0, 255]);

		// 그림 그리기: 다른 캔버스의 일부를 두 배로, 매끄럽게 끄면 가장 가까운 픽셀.
		let mut source_canvas = Canvas2D::new(2, 1);
		source_canvas.set_fill_style("#ff0000");
		source_canvas.fill_rect(0.0, 0.0, 1.0, 1.0);
		source_canvas.set_fill_style("#0000ff");
		source_canvas.fill_rect(1.0, 0.0, 1.0, 1.0);
		let mut target_canvas = Canvas2D::new(8, 4);
		target_canvas.set_image_smoothing_enabled(false);
		target_canvas.draw_image_scaled(&source_canvas, 0.0, 0.0, 8.0, 4.0);
		assert_eq!(read_pixel(&target_canvas, 1, 1), [255, 0, 0, 255]);
		assert_eq!(read_pixel(&target_canvas, 6, 1), [0, 0, 255, 255]);
		target_canvas.clear_rect(0.0, 0.0, 8.0, 4.0);
		target_canvas.draw_image_region(&source_canvas, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 2.0, 2.0);
		assert_eq!(read_pixel(&target_canvas, 1, 1), [0, 0, 255, 255]);
		assert_eq!(read_pixel(&target_canvas, 3, 1)[3], 0);
		let image = source_canvas.to_image();
		target_canvas.draw_image(&image, 4.0, 3.0);
		assert_eq!(read_pixel(&target_canvas, 4, 3), [255, 0, 0, 255]);

		// 패턴: 되풀이.
		let pattern = target_canvas.create_pattern(&source_canvas, CanvasPatternRepetition::Repeat).expect("pattern");
		let mut pattern_canvas = Canvas2D::new(6, 1);
		pattern_canvas.set_fill_style(pattern);
		pattern_canvas.fill_rect(0.0, 0.0, 6.0, 1.0);
		assert_eq!(read_pixel(&pattern_canvas, 2, 0), [255, 0, 0, 255]);
		assert_eq!(read_pixel(&pattern_canvas, 5, 0), [0, 0, 255, 255]);
	}

	//==============================================================================
	/// 글: 엔진 글자 캐시로 칠하고 잰다. (맥에서 시스템 글꼴이 있을 때)
	//==============================================================================
	#[cfg(target_os = "macos")]
	#[test]
	fn text_uses_engine_text_cache() {
		let mut text_string_texture_cache = crate::core::textstringtexturecache::TextStringTextureCache::new();
		let system_font_description = crate::core::textstringtexturecache::parse_font_string(&format!("10px {}", crate::base::platform::SYSTEM_FONT_STRING));
		text_string_texture_cache.set_system_font_chain(system_font_description.family_name_list);
		let mut canvas = Canvas2D::new(120, 40);
		canvas.set_font("24px sans-serif");
		let text_metrics = canvas.measure_text("Hello");
		assert!(text_metrics.width > 30.0);
		canvas.set_fill_style("#000000");
		canvas.set_text_baseline(CanvasTextBaseline::Top);
		canvas.fill_text("Hello", 4.0, 4.0, None);
		let image_data = canvas.get_image_data(0, 0, 120, 40);
		let covered_count = image_data.data.chunks_exact(4).filter(|pixel| pixel[3] > 128).count();
		assert!(covered_count > 50);
		// 좁히기: maxWidth 를 주면 그 폭 안에 든다.
		let mut narrow_canvas = Canvas2D::new(120, 40);
		narrow_canvas.set_font("24px sans-serif");
		narrow_canvas.fill_text("Hello", 0.0, 30.0, Some(20.0));
		let narrow_data = narrow_canvas.get_image_data(24, 0, 96, 40);
		assert_eq!(narrow_data.data.chunks_exact(4).filter(|pixel| pixel[3] > 0).count(), 0);
	}

	//==============================================================================
	/// FNV-1a 64 비트 해시. (크롬에서 읽은 큰 픽셀 묶음을 견주려고)
	//==============================================================================
	fn hash_fnv1a(bytes: &[u8]) -> u64 {
		let mut hash: u64 = 0xcbf29ce484222325;
		for byte in bytes {
			hash ^= *byte as u64;
			hash = hash.wrapping_mul(0x100000001b3);
		}
		hash
	}

	//==============================================================================
	/// 픽셀 쓰기와 읽기: 모든 (색, 알파) 짝이 크롬 158 CPU 캔버스와 같다.
	/// - 픽셀 (x = 색, y = 알파) 에 [색, 255 - 색, (색 x 37 + 11) & 255, 알파] 를 쓴 256 x 256 묶음. 해시는 헤드리스 크롬(--disable-gpu)에서 읽은 값이다.
	/// - 프리멀티플라이 값은 크롬에서 불투명한 검정 위에 drawImage 로 그려 읽었다. (알파 255)
	//==============================================================================
	#[test]
	fn put_and_get_image_data_match_chrome_for_every_alpha() {
		let mut image_data = ImageData::new(256, 256);
		for alpha in 0..256usize {
			for color in 0..256usize {
				let offset = (alpha * 256 + color) * 4;
				image_data.data[offset] = color as u8;
				image_data.data[offset + 1] = (255 - color) as u8;
				image_data.data[offset + 2] = ((color * 37 + 11) & 255) as u8;
				image_data.data[offset + 3] = alpha as u8;
			}
		}
		let mut canvas = Canvas2D::new(256, 256);
		canvas.put_image_data(&image_data, 0, 0);
		let mut premultiplied_pixels = canvas.read_premultiplied_pixels();
		for pixel in premultiplied_pixels.chunks_exact_mut(4) {
			pixel[3] = 255;
		}
		assert_eq!(hash_fnv1a(&premultiplied_pixels), 0xbea87602868fe237);
		assert_eq!(hash_fnv1a(&canvas.get_image_data(0, 0, 256, 256).data), 0x78ee1ba1c57a4355);
		// 반이 딱 떨어지는 값: 크롬은 f32 로 셈해 짝수 쪽(42.5 → 42)이거나 오차로 올라간다(76.5 → 77).
		let mut tie_canvas = Canvas2D::new(2, 1);
		let mut tie_data = ImageData::new(2, 1);
		tie_data.data.copy_from_slice(&[43, 213, 0, 6, 77, 179, 0, 10]);
		tie_canvas.put_image_data(&tie_data, 0, 0);
		assert_eq!(tie_canvas.read_premultiplied_pixels(), vec![1, 5, 0, 6, 3, 7, 0, 10]);
		assert_eq!(tie_canvas.get_image_data(0, 0, 2, 1).data, vec![42, 212, 0, 6, 77, 179, 0, 10]);
	}

	//==============================================================================
	/// 더러운 사각형: 헤드리스 크롬 158 의 putImageData 와 같은 칸에 쓴다.
	/// - 6 x 5 캔버스를 (10, 20, 30) 으로 칠하고 4 x 3 묶음(빨강 100 + 차례)을 쓴 뒤 칸마다 묶음 차례(16진)나 '.'(그대로)를 적었다.
	//==============================================================================
	#[test]
	fn put_image_data_dirty_rect_matches_chrome() {
		const CHROME_DIRTY_TABLE: &[(&[i32], &str)] = &[
			(&[0, 0], "0123..4567..89ab.............."),
			(&[1, 1], ".......0123..4567..89ab......."),
			(&[-1, -1], "567...9ab....................."),
			(&[4, 3], "......................01....45"),
			(&[-3, 0], "3.....7.....b................."),
			(&[5, 4], ".............................0"),
			(&[6, 0], ".............................."),
			(&[0, 5], ".............................."),
			(&[-4, 0], ".............................."),
			(&[i32::MAX, 0], ".............................."),
			(&[i32::MIN, 0], ".............................."),
			(&[0, 0, 1, 1, 2, 1], ".......56....................."),
			(&[1, 1, 1, 0, 2, 2], "........12....56.............."),
			(&[0, 0, 3, 2, -2, -1], ".......56....................."),
			(&[0, 0, -1, -1, 2, 2], "0............................."),
			(&[0, 0, -1, 0, -2, 2], ".............................."),
			(&[2, 1, 0, 0, 0, 0], ".............................."),
			(&[0, 0, 0, 0, -3, 3], ".............................."),
			(&[0, 0, 2, 2, 10, 10], "..............ab.............."),
			(&[0, 0, 5, 0, -3, 3], "..23....67....ab.............."),
			(&[3, 2, -10, -10, 100, 100], "...............012...456...89a"),
			(&[0, 0, 1, 1, -1, -1], "0............................."),
			(&[0, 0, 4, 3, -5, -5], "0123..4567..89ab.............."),
			(&[1, 0, 2, 0, i32::MIN, 3], ".............................."),
			(&[0, 0, i32::MAX, 0, i32::MAX, 3], ".............................."),
			(&[0, 0, 0, 0, 100000, 100000], ".............................."),
			(&[0, 0, 0, 0, 65536, 32768], ".............................."),
			(&[0, 0, 0, 0, 65535, 32768], "0123..4567..89ab.............."),
			(&[0, 0, 2, 0, i32::MIN, 1], "0............................."),
			(&[0, 0, -2, 0, i32::MIN, 1], ".............................."),
			(&[0, 0, 3, 0, -2147483647, 1], "012..........................."),
			(&[1, 1, 3, 2, i32::MIN, 1], "...................89........."),
			(&[0, 0, 1, 2, 2, i32::MIN], ".............................."),
		];
		for (argument_list, expected_cell_text) in CHROME_DIRTY_TABLE {
			let mut canvas = Canvas2D::new(6, 5);
			canvas.set_fill_style("rgb(10, 20, 30)");
			canvas.fill_rect(0.0, 0.0, 6.0, 5.0);
			let mut image_data = canvas.create_image_data(4, 3);
			for index in 0..12 {
				image_data.data[index * 4..index * 4 + 4].copy_from_slice(&[100 + index as u8, 200, index as u8, 255]);
			}
			if argument_list.len() == 2 {
				canvas.put_image_data(&image_data, argument_list[0], argument_list[1]);
			}
			else {
				canvas.put_image_data_region(&image_data, argument_list[0], argument_list[1], argument_list[2], argument_list[3], argument_list[4], argument_list[5]);
			}
			let pixels = canvas.get_image_data(0, 0, 6, 5).data;
			let cell_text: String = pixels.chunks_exact(4).map(|pixel| {
				if pixel[0] == 10 {
					'.'
				}
				else {
					char::from_digit((pixel[0] - 100) as u32, 16).unwrap()
				}
			}).collect();
			assert_eq!(&cell_text, expected_cell_text, "{:?}", argument_list);
		}
	}

	//==============================================================================
	/// 브라우저 new ImageData 가 받지 않는 픽셀 묶음은 쓰지 않는다. (data 길이가 폭 x 높이 x 4 가 아닌 것, 폭이나 높이 0)
	//==============================================================================
	#[test]
	fn put_image_data_skips_invalid_image_data() {
		let mut canvas = Canvas2D::new(2, 1);
		let revision = canvas.revision;
		let long_data = ImageData {
			width: 1,
			height: 1,
			data: vec![255, 0, 0, 255, 0, 255, 0, 255],
		};
		canvas.put_image_data(&long_data, 0, 0);
		let short_data = ImageData {
			width: 2,
			height: 1,
			data: vec![255, 0, 0, 255],
		};
		canvas.put_image_data(&short_data, 0, 0);
		canvas.put_image_data(&ImageData::new(0, 1), 0, 0);
		assert_eq!(canvas.get_image_data(0, 0, 2, 1).data, vec![0; 8]);
		assert_eq!(canvas.revision, revision);
		// 자리가 캔버스 밖이면 바뀐 것이 없다.
		canvas.put_image_data(&ImageData::new(1, 1), 5, 0);
		assert_eq!(canvas.revision, revision);
	}

	//==============================================================================
	/// 글꼴 상태: 틀린 글은 앞 값을 남기고, save 와 restore 를 따르고, reset 은 처음 값. (헤드리스 크롬 158 과 같은 차례)
	//==============================================================================
	#[test]
	fn font_state_follows_chrome() {
		let mut canvas = Canvas2D::new(4, 4);
		assert_eq!(canvas.get_font(), "10px sans-serif");
		canvas.set_font("bogus");
		assert_eq!(canvas.get_font(), "10px sans-serif");
		canvas.set_font("20px a");
		canvas.set_font("bogus");
		assert_eq!(canvas.get_font(), "20px a");
		canvas.save();
		canvas.set_font("30px b");
		assert_eq!(canvas.get_font(), "30px b");
		canvas.restore();
		assert_eq!(canvas.get_font(), "20px a");
		canvas.set_font("oblique 10deg 12px a");
		assert_eq!(canvas.get_font(), "12px a");
		assert_eq!(canvas.state.font.drawing_font, "oblique 12px a");
		canvas.reset();
		assert_eq!(canvas.get_font(), "10px sans-serif");
	}
}
