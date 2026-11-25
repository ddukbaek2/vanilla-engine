//==============================================================================
// 2D 그리기 대상. (원본과 게임이 document.createElement("canvas") 와 getContext("2d") 로 쓰는 브라우저 캔버스 자리)
// - 원본 엔진은 글, 그늘, 그림자를 떨어진 캔버스에 굽고(getBakeCanvas, getAtlasCanvas 가 그 캔버스를 돌려준다),
//   게임은 제 캔버스를 만들어 경로, 그라데이션, 그림자, 자르기, 그림, 글을 그린 뒤 엔진에 그림으로 넘긴다. (윤 나는 UI 껍데기, 로고 아틀라스, 스프라이트 자르기, 픽셀 재기)
// - 러스트 판은 Canvas2D 하나가 캔버스와 그 2D 컨텍스트다. 함수 이름은 CanvasRenderingContext2D 의 이름을 러스트 규칙으로 바꾼 것이다.
//   (fillRect → fill_rect, fillStyle = → set_fill_style, getImageData → get_image_data) Path2D, CanvasGradient, CanvasPattern, ImageData 도 같다.
//   그린 것은 to_image 로 엔진 Image 를 받아 Graphic, UIImageView 같은 곳에 쓴다. (바뀌지 않았으면 같은 Image 를 돌려준다)
// - 웹은 원본처럼 브라우저 캔버스(문서에 붙이지 않은 canvas 요소)와 그 2D 컨텍스트에 그대로 그린다.
// - 데스크탑은 브라우저가 없어 엔진이 같은 일을 한다. 크롬이 쓰는 Skia 의 CPU 래스터라이저를 러스트로 옮긴 tiny-skia 로 칠한다.
//   (resvg 가 이미 쓰는 같은 판, 더 받는 크레이트가 없다) 경로 점은 원본처럼 더할 때의 변환으로 옮기고, 그라데이션과 패턴은 칠할 때의 변환을 따른다.
//   경로의 arc, ellipse, arcTo, roundRect 는 크롬처럼 블링크 canvas_path.cc 와 스키아 SkPathBuilder 의 셈으로 같은 원뿔 곡선을 담고,
//   칠할 때 스키아 CPU 래스터처럼 기기 픽셀 0.25 안의 2 차 곡선으로 나눈다. (SkAutoConicToQuads, 크롬이 쓰는 옛 판 chop)
//   GPU 에 올리며 CPU 픽셀을 놓은 엔진 Image 는 그 텍스처를 되읽어 그린다. (크롬이 GPU 에 있는 그림을 CPU 캔버스에 그릴 때와 같다)
//   그림자는 모양을 따로 칠해 흐리고(시그마 = shadowBlur / 2, 상자 흐림 세 번) 변환을 받지 않는 shadowOffset 만큼 옮겨 깐다.
//   copy, source-in, source-out, destination-in, destination-atop 은 원본처럼 모양 밖도 지운다. (모양을 따로 칠해 자른 영역 전체에 합친다)
//   글은 엔진 글자 캐시(Text 컴포넌트와 같은 얼굴 고르기, 모양 짓기, 대체 글꼴, 색 이모지)로 덮인 정도를 받아 칠한다.
// - 원본과 다른 점:
//   - 속성은 setter 와 getter 다. 색은 Color 나 CSS 색 글, 그라데이션, 패턴을 받는 CanvasStyle 이다.
//   - roundRect 의 반지름은 수(f32)나 타원 모서리((x, y), CanvasRadius::Elliptical, 원본 DOMPointInit)의 목록이다.
//   - 데스크탑 픽셀은 크롬과 같은 Skia 셈이지만 같은 판이 아니라 가장자리 안티에일리어싱과 흐림이 조금 다를 수 있다.
//==============================================================================
use crate::base::color::Color;
use std::f32::consts::PI;

//==============================================================================
// 2D 변환 행렬. (원본 DOMMatrix 의 a, b, c, d, e, f. x' = a x + c y + e, y' = b x + d y + f)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasMatrix {
	pub a: f32,
	pub b: f32,
	pub c: f32,
	pub d: f32,
	pub e: f32,
	pub f: f32,
}

impl CanvasMatrix {
	//==============================================================================
	// 단위 행렬.
	//==============================================================================
	pub const IDENTITY: CanvasMatrix = CanvasMatrix {
		a: 1.0,
		b: 0.0,
		c: 0.0,
		d: 1.0,
		e: 0.0,
		f: 0.0,
	};

	//==============================================================================
	// 여섯 값으로 만들기.
	//==============================================================================
	pub fn create(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> CanvasMatrix {
		CanvasMatrix {
			a,
			b,
			c,
			d,
			e,
			f,
		}
	}

	//==============================================================================
	// 뒤에 곱하기. (self × other, other 를 먼저 적용한다. 원본 transform 의 셈)
	//==============================================================================
	pub fn multiply(&self, other: &CanvasMatrix) -> CanvasMatrix {
		CanvasMatrix {
			a: self.a * other.a + self.c * other.b,
			b: self.b * other.a + self.d * other.b,
			c: self.a * other.c + self.c * other.d,
			d: self.b * other.c + self.d * other.d,
			e: self.a * other.e + self.c * other.f + self.e,
			f: self.b * other.e + self.d * other.f + self.f,
		}
	}

	//==============================================================================
	// 점 옮기기.
	//==============================================================================
	pub fn transform_point(&self, x: f32, y: f32) -> (f32, f32) {
		(self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
	}

	//==============================================================================
	// 역행렬. (뒤집을 수 없으면 None)
	//==============================================================================
	pub fn invert(&self) -> Option<CanvasMatrix> {
		let determinant = self.a * self.d - self.b * self.c;
		if determinant == 0.0 || !determinant.is_finite() {
			return None;
		}
		let inverse_determinant = 1.0 / determinant;
		Some(CanvasMatrix {
			a: self.d * inverse_determinant,
			b: -self.b * inverse_determinant,
			c: -self.c * inverse_determinant,
			d: self.a * inverse_determinant,
			e: (self.c * self.f - self.d * self.e) * inverse_determinant,
			f: (self.b * self.e - self.a * self.f) * inverse_determinant,
		})
	}

	//==============================================================================
	// 모든 값이 유한한지.
	//==============================================================================
	pub fn is_finite(&self) -> bool {
		self.a.is_finite() && self.b.is_finite() && self.c.is_finite() && self.d.is_finite() && self.e.is_finite() && self.f.is_finite()
	}
}

impl Default for CanvasMatrix {
	fn default() -> CanvasMatrix {
		CanvasMatrix::IDENTITY
	}
}

//==============================================================================
// 경로 조각. (점은 경로가 든 좌표계. 원본 크롬처럼 arc, ellipse, arcTo, roundRect 는 스키아 원뿔 곡선(conic)으로 둔다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathSegment {
	MoveTo(f32, f32),
	LineTo(f32, f32),
	QuadraticTo(f32, f32, f32, f32),
	CubicTo(f32, f32, f32, f32, f32, f32),
	// 원뿔 곡선. (조정점, 끝 점, 무게)
	ConicTo(f32, f32, f32, f32, f32),
	Close,
}

//==============================================================================
// 둥근 사각형 모서리 반지름. (원본 roundRect 의 수 또는 DOMPointInit {x, y})
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasRadius {
	Circular(f32),
	Elliptical {
		x: f32,
		y: f32,
	},
}

impl From<f32> for CanvasRadius {
	fn from(radius: f32) -> CanvasRadius {
		CanvasRadius::Circular(radius)
	}
}

impl From<(f32, f32)> for CanvasRadius {
	fn from(radius: (f32, f32)) -> CanvasRadius {
		CanvasRadius::Elliptical {
			x: radius.0,
			y: radius.1,
		}
	}
}

//==============================================================================
// 브라우저 Path2D 에 다시 넣을 명령. (웹은 원본처럼 브라우저 Path2D 의 함수를 그대로 부른다)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) enum BrowserPathCommand {
	MoveTo(f32, f32),
	LineTo(f32, f32),
	QuadraticCurveTo(f32, f32, f32, f32),
	BezierCurveTo(f32, f32, f32, f32, f32, f32),
	Arc(f32, f32, f32, f32, f32, bool),
	ArcTo(f32, f32, f32, f32, f32),
	Ellipse(f32, f32, f32, f32, f32, f32, f32, bool),
	Rect(f32, f32, f32, f32),
	RoundRect(f32, f32, f32, f32, Vec<CanvasRadius>),
	ClosePath,
	AddPath(Vec<BrowserPathCommand>, Option<CanvasMatrix>),
}

// 스키아 수. (SkScalar.h: SK_ScalarNearlyZero, SK_ScalarSinCosNearlyZero, SK_ScalarRoot2Over2, SK_FloatPI)
const SKIA_SCALAR_NEARLY_ZERO: f32 = 1.0 / 4096.0;
const SKIA_SIN_COS_NEARLY_ZERO: f32 = 1.0 / 65536.0;
const SKIA_ROOT2_OVER2: f32 = 0.707106781;
const SKIA_FLOAT_PI: f32 = 3.14159265;
// 스키아 conic 을 2 차 곡선으로 나누는 가장 큰 2 의 거듭제곱. (SkGeometry.h kMaxConicToQuadPOW2)
#[cfg(not(target_arch = "wasm32"))]
const SKIA_MAXIMUM_CONIC_TO_QUAD_POWER: u32 = 5;
// 블링크 각. (math_extras.h kTwoPiFloat, kPiOverTwoFloat)
const BLINK_TWO_PI_FLOAT: f32 = PI * 2.0;
const BLINK_PI_OVER_TWO_FLOAT: f32 = PI / 2.0;

//==============================================================================
// 경로. (원본 Path2D, 캔버스의 지금 경로도 이것이다)
// - 더하는 함수의 점은 matrix 로 옮겨 담는다. Path2D 는 단위 행렬이고, 캔버스의 지금 경로는 더할 때의 변환이다. (원본과 같다)
// - arc, ellipse, arcTo, roundRect 는 블링크 canvas_path.cc 와 스키아 SkPathBuilder 의 셈을 옮겨 같은 원뿔 곡선 점과 무게를 담는다.
//   (원뿔 곡선은 아핀 변환을 지나도 무게가 같아 옮겨 담은 점도 같다) 칠할 때는 스키아 CPU 래스터처럼 기기 픽셀 0.25 안의 2 차 곡선으로 나눈다.
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path2D {
	segment_list: Vec<PathSegment>,
	// 마지막 점과 지금 조각의 시작 점. (담은 좌표계, 조각이 없으면 None)
	current_point: Option<(f32, f32)>,
	subpath_start_point: Option<(f32, f32)>,
	// 브라우저 Path2D 에 다시 넣을 명령. (웹)
	#[cfg(target_arch = "wasm32")]
	browser_command_list: Vec<BrowserPathCommand>,
}

impl Path2D {
	//==============================================================================
	// 빈 경로.
	//==============================================================================
	pub fn new() -> Path2D {
		Path2D::default()
	}

	//==============================================================================
	// 조각 목록. (데스크탑 캔버스가 칠할 때)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn get_segment_list(&self) -> &[PathSegment] {
		&self.segment_list
	}

	//==============================================================================
	// 브라우저 Path2D 에 다시 넣을 명령. (웹)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	pub(crate) fn get_browser_command_list(&self) -> &[BrowserPathCommand] {
		&self.browser_command_list
	}

	//==============================================================================
	// 브라우저 명령 적기. (웹만 든다)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn record_browser_command(&mut self, command: BrowserPathCommand) {
		self.browser_command_list.push(command);
	}

	//==============================================================================
	// 브라우저 명령 적기. (데스크탑은 버린다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	fn record_browser_command(&mut self, _command: BrowserPathCommand) {
	}

	//==============================================================================
	// 비었는지.
	//==============================================================================
	pub fn is_empty(&self) -> bool {
		self.segment_list.is_empty()
	}

	//==============================================================================
	// 모두 지우기. (원본 beginPath, 데스크탑 캔버스의 지금 경로)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub(crate) fn clear(&mut self) {
		self.segment_list.clear();
		self.current_point = None;
		self.subpath_start_point = None;
	}

	//==============================================================================
	// 새 조각. (원본 moveTo)
	//==============================================================================
	pub fn move_to(&mut self, x: f32, y: f32) {
		self.move_to_with(&CanvasMatrix::IDENTITY, x, y);
		self.record_browser_command(BrowserPathCommand::MoveTo(x, y));
	}

	//==============================================================================
	// 곧은 선. (원본 lineTo, 조각이 없으면 그 점에서 조각을 시작한다)
	//==============================================================================
	pub fn line_to(&mut self, x: f32, y: f32) {
		self.line_to_with(&CanvasMatrix::IDENTITY, x, y);
		self.record_browser_command(BrowserPathCommand::LineTo(x, y));
	}

	//==============================================================================
	// 2 차 곡선. (원본 quadraticCurveTo)
	//==============================================================================
	pub fn quadratic_curve_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
		self.quadratic_curve_to_with(&CanvasMatrix::IDENTITY, control_x, control_y, x, y);
		self.record_browser_command(BrowserPathCommand::QuadraticCurveTo(control_x, control_y, x, y));
	}

	//==============================================================================
	// 3 차 곡선. (원본 bezierCurveTo)
	//==============================================================================
	pub fn bezier_curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		self.bezier_curve_to_with(&CanvasMatrix::IDENTITY, first_control_x, first_control_y, second_control_x, second_control_y, x, y);
		self.record_browser_command(BrowserPathCommand::BezierCurveTo(first_control_x, first_control_y, second_control_x, second_control_y, x, y));
	}

	//==============================================================================
	// 원호. (원본 arc, 반지름이 음수면 원본의 예외처럼 아무것도 하지 않고 false)
	//==============================================================================
	pub fn arc(&mut self, x: f32, y: f32, radius: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		let is_accepted = self.arc_with(&CanvasMatrix::IDENTITY, x, y, radius, start_angle, end_angle, is_counterclockwise);
		if is_accepted {
			self.record_browser_command(BrowserPathCommand::Arc(x, y, radius, start_angle, end_angle, is_counterclockwise));
		}
		is_accepted
	}

	//==============================================================================
	// 두 접선 사이 원호. (원본 arcTo, 반지름이 음수면 false)
	//==============================================================================
	pub fn arc_to(&mut self, first_x: f32, first_y: f32, second_x: f32, second_y: f32, radius: f32) -> bool {
		let is_accepted = self.arc_to_with(&CanvasMatrix::IDENTITY, first_x, first_y, second_x, second_y, radius);
		if is_accepted {
			self.record_browser_command(BrowserPathCommand::ArcTo(first_x, first_y, second_x, second_y, radius));
		}
		is_accepted
	}

	//==============================================================================
	// 타원호. (원본 ellipse, 반지름이 음수면 false)
	//==============================================================================
	pub fn ellipse(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		let is_accepted = self.ellipse_with(&CanvasMatrix::IDENTITY, x, y, radius_x, radius_y, rotation, start_angle, end_angle, is_counterclockwise);
		if is_accepted {
			self.record_browser_command(BrowserPathCommand::Ellipse(x, y, radius_x, radius_y, rotation, start_angle, end_angle, is_counterclockwise));
		}
		is_accepted
	}

	//==============================================================================
	// 사각형. (원본 rect, 닫힌 조각을 더하고 (x, y) 에서 새 조각)
	//==============================================================================
	pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.rect_with(&CanvasMatrix::IDENTITY, x, y, width, height);
		self.record_browser_command(BrowserPathCommand::Rect(x, y, width, height));
	}

	//==============================================================================
	// 둥근 사각형. (원본 roundRect, 반지름 1 ~ 4 개. 수(f32)나 타원 모서리((x, y), CanvasRadius::Elliptical)를 받는다. 틀리면 false)
	//==============================================================================
	pub fn round_rect<R: Into<CanvasRadius> + Copy>(&mut self, x: f32, y: f32, width: f32, height: f32, radius_list: &[R]) -> bool {
		let radius_list: Vec<CanvasRadius> = radius_list.iter().map(|radius| (*radius).into()).collect();
		let is_accepted = self.round_rect_with(&CanvasMatrix::IDENTITY, x, y, width, height, &radius_list);
		if is_accepted {
			self.record_browser_command(BrowserPathCommand::RoundRect(x, y, width, height, radius_list));
		}
		is_accepted
	}

	//==============================================================================
	// 조각 닫기. (원본 closePath)
	//==============================================================================
	pub fn close_path(&mut self) {
		self.close_subpath();
		self.record_browser_command(BrowserPathCommand::ClosePath);
	}

	//==============================================================================
	// 다른 경로 더하기. (원본 addPath, 변환을 주면 그 변환으로 옮겨 더한다)
	//==============================================================================
	pub fn add_path(&mut self, path: &Path2D, transform: Option<CanvasMatrix>) {
		let matrix = transform.unwrap_or(CanvasMatrix::IDENTITY);
		if !matrix.is_finite() {
			return;
		}
		for segment in path.segment_list.iter() {
			match *segment {
				PathSegment::MoveTo(x, y) => {
					self.move_to_with(&matrix, x, y);
				}
				PathSegment::LineTo(x, y) => {
					self.push_line(&matrix, x, y);
				}
				PathSegment::QuadraticTo(control_x, control_y, x, y) => {
					self.push_quadratic(&matrix, control_x, control_y, x, y);
				}
				PathSegment::CubicTo(first_control_x, first_control_y, second_control_x, second_control_y, x, y) => {
					self.push_cubic(&matrix, first_control_x, first_control_y, second_control_x, second_control_y, x, y);
				}
				PathSegment::ConicTo(control_x, control_y, x, y, weight) => {
					self.push_conic(&matrix, (control_x, control_y), (x, y), weight);
				}
				PathSegment::Close => {
					self.close_subpath();
				}
			}
		}
		#[cfg(target_arch = "wasm32")]
		{
			let command_list = path.browser_command_list.clone();
			self.record_browser_command(BrowserPathCommand::AddPath(command_list, transform));
		}
	}

	//==============================================================================
	// 점 옮겨 담기. (유한하지 않으면 원본처럼 그 조각을 버린다)
	//==============================================================================
	fn map_point(matrix: &CanvasMatrix, x: f32, y: f32) -> Option<(f32, f32)> {
		if !x.is_finite() || !y.is_finite() {
			return None;
		}
		let (mapped_x, mapped_y) = matrix.transform_point(x, y);
		if !mapped_x.is_finite() || !mapped_y.is_finite() {
			return None;
		}
		Some((mapped_x, mapped_y))
	}

	//==============================================================================
	// 조각이 없으면 그 점에서 시작한다. (원본 "ensure there is a subpath")
	//==============================================================================
	fn ensure_subpath(&mut self, matrix: &CanvasMatrix, x: f32, y: f32) {
		if self.current_point.is_none() {
			self.move_to_with(matrix, x, y);
		}
	}

	//==============================================================================
	// moveTo. (옮겨 담는 행렬과 함께)
	//==============================================================================
	pub(crate) fn move_to_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32) {
		if let Some(point) = Path2D::map_point(matrix, x, y) {
			self.segment_list.push(PathSegment::MoveTo(point.0, point.1));
			self.current_point = Some(point);
			self.subpath_start_point = Some(point);
		}
	}

	//==============================================================================
	// 곧은 선 담기. (조각이 있다고 본다)
	//==============================================================================
	fn push_line(&mut self, matrix: &CanvasMatrix, x: f32, y: f32) {
		if let Some(point) = Path2D::map_point(matrix, x, y) {
			if self.current_point.is_none() {
				self.segment_list.push(PathSegment::MoveTo(point.0, point.1));
				self.subpath_start_point = Some(point);
			}
			else {
				self.segment_list.push(PathSegment::LineTo(point.0, point.1));
			}
			self.current_point = Some(point);
		}
	}

	//==============================================================================
	// 2 차 곡선 담기.
	//==============================================================================
	fn push_quadratic(&mut self, matrix: &CanvasMatrix, control_x: f32, control_y: f32, x: f32, y: f32) {
		if let (Some(control), Some(point)) = (Path2D::map_point(matrix, control_x, control_y), Path2D::map_point(matrix, x, y)) {
			self.segment_list.push(PathSegment::QuadraticTo(control.0, control.1, point.0, point.1));
			self.current_point = Some(point);
		}
	}

	//==============================================================================
	// 3 차 곡선 담기.
	//==============================================================================
	fn push_cubic(&mut self, matrix: &CanvasMatrix, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		if let (Some(first_control), Some(second_control), Some(point)) = (Path2D::map_point(matrix, first_control_x, first_control_y), Path2D::map_point(matrix, second_control_x, second_control_y), Path2D::map_point(matrix, x, y)) {
			self.segment_list.push(PathSegment::CubicTo(first_control.0, first_control.1, second_control.0, second_control.1, point.0, point.1));
			self.current_point = Some(point);
		}
	}

	//==============================================================================
	// 원뿔 곡선 담기. (스키아 SkPathBuilder::conicTo: 무게가 0 이하면 끝 점으로 곧은 선, 1 이면 2 차 곡선, 유한하지 않으면 곧은 선 둘)
	//==============================================================================
	fn push_conic(&mut self, matrix: &CanvasMatrix, control: (f32, f32), end: (f32, f32), weight: f32) {
		if self.current_point.is_none() {
			self.move_to_with(matrix, 0.0, 0.0);
		}
		if !(weight > 0.0) {
			self.push_line(matrix, end.0, end.1);
		}
		else if weight == 1.0 {
			self.push_quadratic(matrix, control.0, control.1, end.0, end.1);
		}
		else if weight.is_finite() {
			if let (Some(mapped_control), Some(mapped_end)) = (Path2D::map_point(matrix, control.0, control.1), Path2D::map_point(matrix, end.0, end.1)) {
				self.segment_list.push(PathSegment::ConicTo(mapped_control.0, mapped_control.1, mapped_end.0, mapped_end.1, weight));
				self.current_point = Some(mapped_end);
			}
		}
		else {
			self.push_line(matrix, control.0, control.1);
			self.push_line(matrix, end.0, end.1);
		}
	}

	//==============================================================================
	// 조각 닫기. (안쪽, 브라우저 명령은 적지 않는다)
	//==============================================================================
	fn close_subpath(&mut self) {
		if self.current_point.is_none() {
			return;
		}
		self.segment_list.push(PathSegment::Close);
		self.current_point = self.subpath_start_point;
	}

	//==============================================================================
	// lineTo. (옮겨 담는 행렬과 함께)
	//==============================================================================
	pub(crate) fn line_to_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32) {
		self.ensure_subpath(matrix, x, y);
		self.push_line(matrix, x, y);
	}

	//==============================================================================
	// quadraticCurveTo. (옮겨 담는 행렬과 함께)
	//==============================================================================
	pub(crate) fn quadratic_curve_to_with(&mut self, matrix: &CanvasMatrix, control_x: f32, control_y: f32, x: f32, y: f32) {
		self.ensure_subpath(matrix, control_x, control_y);
		self.push_quadratic(matrix, control_x, control_y, x, y);
	}

	//==============================================================================
	// bezierCurveTo. (옮겨 담는 행렬과 함께)
	//==============================================================================
	pub(crate) fn bezier_curve_to_with(&mut self, matrix: &CanvasMatrix, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		self.ensure_subpath(matrix, first_control_x, first_control_y);
		self.push_cubic(matrix, first_control_x, first_control_y, second_control_x, second_control_y, x, y);
	}

	//==============================================================================
	// rect. (옮겨 담는 행렬과 함께. 블링크: 폭과 높이가 모두 0 이면 moveTo 만)
	//==============================================================================
	pub(crate) fn rect_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32, width: f32, height: f32) {
		if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) {
			return;
		}
		if width == 0.0 && height == 0.0 {
			self.move_to_with(matrix, x, y);
			return;
		}
		self.move_to_with(matrix, x, y);
		self.push_line(matrix, x + width, y);
		self.push_line(matrix, x + width, y + height);
		self.push_line(matrix, x, y + height);
		self.close_subpath();
	}

	//==============================================================================
	// arc. (옮겨 담는 행렬과 함께. 블링크 CanvasPath::arc)
	// - 반지름이 0 이거나 시작 각과 끝 각이 같으면 시작 점까지 곧은 선만. 각을 다듬고(시작 각을 [0, 2π) 로) 끝 각을 맞춘 뒤 원뿔 곡선으로 담는다.
	//==============================================================================
	pub(crate) fn arc_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32, radius: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		if ![x, y, radius, start_angle, end_angle].iter().all(|value| value.is_finite()) {
			return true;
		}
		if radius < 0.0 {
			return false;
		}
		if matrix.invert().is_none() {
			return true;
		}
		if radius == 0.0 || start_angle == end_angle {
			self.line_to_with(matrix, x + radius * start_angle.cos(), y + radius * start_angle.sin());
			return true;
		}
		let (start_angle, end_angle) = canonicalize_angle(start_angle, end_angle);
		let end_angle = adjust_end_angle(start_angle, end_angle, is_counterclockwise);
		self.add_ellipse(matrix, x, y, radius, radius, 0.0, start_angle, end_angle);
		true
	}

	//==============================================================================
	// ellipse. (옮겨 담는 행렬과 함께. 블링크 CanvasPath::ellipse)
	// - 반지름 하나가 0 이거나 다듬은 각이 같으면 시작 점, 0.5π 마다의 끝점, 끝 점을 곧은 선으로 잇는다. (DegenerateEllipse)
	//==============================================================================
	pub(crate) fn ellipse_with(&mut self, matrix: &CanvasMatrix, center_x: f32, center_y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		if ![center_x, center_y, radius_x, radius_y, rotation, start_angle, end_angle].iter().all(|value| value.is_finite()) {
			return true;
		}
		if radius_x < 0.0 || radius_y < 0.0 {
			return false;
		}
		if matrix.invert().is_none() {
			return true;
		}
		let (start_angle, end_angle) = canonicalize_angle(start_angle, end_angle);
		let adjusted_end_angle = adjust_end_angle(start_angle, end_angle, is_counterclockwise);
		if radius_x == 0.0 || radius_y == 0.0 || start_angle == adjusted_end_angle {
			self.add_degenerate_ellipse(matrix, center_x, center_y, radius_x, radius_y, rotation, start_angle, adjusted_end_angle, is_counterclockwise);
			return true;
		}
		self.add_ellipse(matrix, center_x, center_y, radius_x, radius_y, rotation, start_angle, adjusted_end_angle);
		true
	}

	//==============================================================================
	// 납작한 타원. (블링크 DegenerateEllipse: 곧은 선으로 시작 점, 0.5π 마다의 끝점, 끝 점)
	//==============================================================================
	fn add_degenerate_ellipse(&mut self, matrix: &CanvasMatrix, center_x: f32, center_y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) {
		let rotation_cosine = (rotation as f64).cos();
		let rotation_sine = (rotation as f64).sin();
		let map_angle = |angle: f32| -> (f32, f32) {
			let point_x = (radius_x * angle.cos()) as f64;
			let point_y = (radius_y * angle.sin()) as f64;
			(center_x + (point_x * rotation_cosine - point_y * rotation_sine) as f32, center_y + (point_x * rotation_sine + point_y * rotation_cosine) as f32)
		};
		let start_point = map_angle(start_angle);
		self.line_to_with(matrix, start_point.0, start_point.1);
		if (radius_x == 0.0 && radius_y == 0.0) || start_angle == end_angle {
			return;
		}
		if !is_counterclockwise {
			let mut angle = start_angle - start_angle % BLINK_PI_OVER_TWO_FLOAT + BLINK_PI_OVER_TWO_FLOAT;
			while angle < end_angle {
				let point = map_angle(angle);
				self.line_to_with(matrix, point.0, point.1);
				angle += BLINK_PI_OVER_TWO_FLOAT;
			}
		}
		else {
			let mut angle = start_angle - start_angle % BLINK_PI_OVER_TWO_FLOAT;
			while angle > end_angle {
				let point = map_angle(angle);
				self.line_to_with(matrix, point.0, point.1);
				angle -= BLINK_PI_OVER_TWO_FLOAT;
			}
		}
		let end_point = map_angle(end_angle);
		self.line_to_with(matrix, end_point.0, end_point.1);
	}

	//==============================================================================
	// 타원호 담기. (블링크 PathBuilder::AddEllipse: 각을 도로 바꾸고, 한 바퀴는 반 바퀴 둘로. 돌린 타원은 타원 좌표계의 원점 타원)
	//==============================================================================
	fn add_ellipse(&mut self, matrix: &CanvasMatrix, center_x: f32, center_y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32) {
		let (ellipse_matrix, oval) = if rotation != 0.0 {
			let ellipse_matrix = matrix.multiply(&CanvasMatrix::create(rotation.cos(), rotation.sin(), -rotation.sin(), rotation.cos(), center_x, center_y));
			(ellipse_matrix, (-radius_x, -radius_y, radius_x, radius_y))
		}
		else {
			(*matrix, (center_x - radius_x, center_y - radius_y, center_x + radius_x, center_y + radius_y))
		};
		let degrees_per_radian = 180.0 / PI;
		let start_degrees = start_angle * degrees_per_radian;
		let sweep_degrees = (end_angle - start_angle) * degrees_per_radian;
		// SkPathBuilder::arcTo(oval, angle, 360, false) 는 아무것도 그리지 않아 180 둘로 나눈다.
		if (sweep_degrees.abs() - 360.0).abs() <= SKIA_SCALAR_NEARLY_ZERO {
			let sweep_half = 180.0f32.copysign(sweep_degrees);
			self.arc_to_oval(&ellipse_matrix, oval, start_degrees, sweep_half, false);
			self.arc_to_oval(&ellipse_matrix, oval, start_degrees + sweep_half, sweep_half, false);
		}
		else {
			self.arc_to_oval(&ellipse_matrix, oval, start_degrees, sweep_degrees, false);
		}
	}

	//==============================================================================
	// 타원 위 원호를 원뿔 곡선으로. (스키아 SkPathBuilder::arcTo(oval, startAngle, sweepAngle, forceMoveTo), 각은 도)
	//==============================================================================
	fn arc_to_oval(&mut self, matrix: &CanvasMatrix, oval: (f32, f32, f32, f32), start_angle: f32, sweep_angle: f32, force_move_to: bool) {
		let (left, top, right, bottom) = oval;
		let oval_width = right - left;
		let oval_height = bottom - top;
		if oval_width < 0.0 || oval_height < 0.0 {
			return;
		}
		let start_angle = start_angle % 360.0;
		let force_move_to = force_move_to || self.segment_list.is_empty();
		let center_x = ((left as f64 + right as f64) * 0.5) as f32;
		let center_y = ((top as f64 + bottom as f64) * 0.5) as f32;
		// 한 점뿐인 원호.
		let lone_point = if sweep_angle == 0.0 && (start_angle == 0.0 || start_angle == 360.0) {
			Some((right, center_y))
		}
		else if oval_width == 0.0 && oval_height == 0.0 {
			Some((right, top))
		}
		else {
			None
		};
		if let Some(lone_point) = lone_point {
			if force_move_to {
				self.move_to_with(matrix, lone_point.0, lone_point.1);
			}
			else {
				self.line_to_with(matrix, lone_point.0, lone_point.1);
			}
			return;
		}
		let (start_vector, stop_vector, is_clockwise) = convert_angles_to_unit_vectors(start_angle, sweep_angle);
		let add_point = |path: &mut Path2D, point: (f32, f32)| {
			if force_move_to {
				path.move_to_with(matrix, point.0, point.1);
				return;
			}
			let is_nearly_equal = match (path.current_point, Path2D::map_point(matrix, point.0, point.1)) {
				(Some(current_point), Some(mapped_point)) => {
					(current_point.0 - mapped_point.0).abs() <= SKIA_SCALAR_NEARLY_ZERO && (current_point.1 - mapped_point.1).abs() <= SKIA_SCALAR_NEARLY_ZERO
				}
				_ => {
					false
				}
			};
			if !is_nearly_equal {
				path.line_to_with(matrix, point.0, point.1);
			}
		};
		let radius_x = oval_width / 2.0;
		let radius_y = oval_height / 2.0;
		// 쓸어 가는 각이 너무 작아 단위 벡터가 같으면 끝 점까지 곧은 선.
		if start_vector == stop_vector {
			let end_angle = (start_angle + sweep_angle) * (SKIA_FLOAT_PI / 180.0);
			add_point(self, (center_x + radius_x * end_angle.cos(), center_y + radius_y * end_angle.sin()));
			return;
		}
		let map_unit_point = |point: (f32, f32)| -> (f32, f32) {
			(point.0 * radius_x + center_x, point.1 * radius_y + center_y)
		};
		let conic_list = build_unit_arc(start_vector, stop_vector, is_clockwise);
		if conic_list.is_empty() {
			add_point(self, map_unit_point(stop_vector));
			return;
		}
		add_point(self, map_unit_point(conic_list[0].0[0]));
		for (points, weight) in conic_list.iter() {
			self.push_conic(matrix, map_unit_point(points[1]), map_unit_point(points[2]), *weight);
		}
	}

	//==============================================================================
	// arcTo. (옮겨 담는 행렬과 함께. 블링크 CanvasPath::arcTo 와 스키아 SkPathBuilder::arcTo(p1, p2, radius), 지금 점은 역행렬로 되돌려 셈한다)
	// - 지금 점이 없으면 (x1, y1) 로 moveTo, 그 점이 지금 점이거나 두 점이 같거나 반지름이 0 이면 곧은 선.
	// - 두 접선의 방향은 배정밀도로 셈하고, 첫 접점까지 곧은 선을 그은 뒤 무게 sqrt((1 + cos) / 2) 의 원뿔 곡선 하나.
	//==============================================================================
	pub(crate) fn arc_to_with(&mut self, matrix: &CanvasMatrix, first_x: f32, first_y: f32, second_x: f32, second_y: f32, radius: f32) -> bool {
		if ![first_x, first_y, second_x, second_y, radius].iter().all(|value| value.is_finite()) {
			return true;
		}
		if radius < 0.0 {
			return false;
		}
		let (current_x, current_y) = match (self.current_point, matrix.invert()) {
			(Some(current_point), Some(inverse_matrix)) => {
				inverse_matrix.transform_point(current_point.0, current_point.1)
			}
			(None, _) => {
				self.move_to_with(matrix, first_x, first_y);
				return true;
			}
			(Some(_), None) => {
				return true;
			}
		};
		if (first_x == current_x && first_y == current_y) || (first_x == second_x && first_y == second_y) || radius == 0.0 {
			self.line_to_with(matrix, first_x, first_y);
			return true;
		}
		let normalize = |vector_x: f64, vector_y: f64| -> (f64, f64) {
			let length = (vector_x * vector_x + vector_y * vector_y).sqrt();
			(vector_x / length, vector_y / length)
		};
		let before = normalize((first_x - current_x) as f64, (first_y - current_y) as f64);
		let after = normalize((second_x - first_x) as f64, (second_y - first_y) as f64);
		let cosine = before.0 * after.0 + before.1 * after.1;
		let sine = before.0 * after.1 - before.1 * after.0;
		let is_finite = before.0.is_finite() && before.1.is_finite() && after.0.is_finite() && after.1.is_finite();
		if !is_finite || (sine as f32).abs() <= SKIA_SCALAR_NEARLY_ZERO {
			self.line_to_with(matrix, first_x, first_y);
			return true;
		}
		let distance = ((radius as f64 * (1.0 - cosine) / sine) as f32).abs();
		let tangent_x = (first_x as f64 - distance as f64 * before.0) as f32;
		let tangent_y = (first_y as f64 - distance as f64 * before.1) as f32;
		let after_vector = set_vector_length((after.0 as f32, after.1 as f32), distance);
		self.line_to_with(matrix, tangent_x, tangent_y);
		let weight = ((0.5 + cosine * 0.5) as f32).sqrt();
		self.push_conic(matrix, (first_x, first_y), (first_x + after_vector.0, first_y + after_vector.1), weight);
		true
	}

	//==============================================================================
	// roundRect. (옮겨 담는 행렬과 함께. 블링크 CanvasPath::roundRect 와 스키아 SkRRect, SkPathBuilder::addRRect)
	// - 반지름 1 개는 모두, 2 개는 [왼위와 오른아래, 오른위와 왼아래], 3 개는 [왼위, 오른위와 왼아래, 오른아래], 4 개는 [왼위, 오른위, 오른아래, 왼아래].
	// - 폭이나 높이가 0 이면 사각형만. 음수 폭과 높이는 뒤집고 방향을 바꾼다. 넘치는 반지름은 x 와 y 를 같은 비로 줄인다. (스키아 scaleRadii)
	// - 시작은 왼위 모서리 곡선 뒤, 블링크처럼 끝에 뒤집은 (x, y) 에서 새 조각.
	//==============================================================================
	pub(crate) fn round_rect_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32, width: f32, height: f32, radius_list: &[CanvasRadius]) -> bool {
		if radius_list.is_empty() || radius_list.len() > 4 {
			return false;
		}
		if matrix.invert().is_none() {
			return true;
		}
		if ![x, y, width, height].iter().all(|value| value.is_finite()) {
			return true;
		}
		let mut size_list: Vec<(f32, f32)> = Vec::with_capacity(radius_list.len());
		for radius in radius_list {
			let size = match *radius {
				CanvasRadius::Circular(radius) => {
					(radius, radius)
				}
				CanvasRadius::Elliptical {
					x: radius_x,
					y: radius_y,
				} => {
					(radius_x, radius_y)
				}
			};
			if !size.0.is_finite() || !size.1.is_finite() {
				return true;
			}
			if size.0 < 0.0 || size.1 < 0.0 {
				return false;
			}
			size_list.push(size);
		}
		if width == 0.0 || height == 0.0 {
			self.add_rect_points(matrix, (x, y, x + width, y + height), true);
			return true;
		}
		// 블링크 차례: 왼위, 오른위, 왼아래, 오른아래.
		let mut corner_list: [(f32, f32); 4] = match size_list.len() {
			1 => {
				[size_list[0]; 4]
			}
			2 => {
				[size_list[0], size_list[1], size_list[1], size_list[0]]
			}
			3 => {
				[size_list[0], size_list[1], size_list[1], size_list[2]]
			}
			_ => {
				[size_list[0], size_list[1], size_list[3], size_list[2]]
			}
		};
		let mut left = x;
		let mut top = y;
		let mut rect_width = width;
		let mut rect_height = height;
		let mut is_clockwise = true;
		if rect_width < 0.0 {
			is_clockwise = false;
			left += rect_width;
			rect_width = -rect_width;
			corner_list.swap(0, 1);
			corner_list.swap(2, 3);
		}
		if rect_height < 0.0 {
			is_clockwise = !is_clockwise;
			top += rect_height;
			rect_height = -rect_height;
			corner_list.swap(0, 2);
			corner_list.swap(1, 3);
		}
		let rect = (left, top, left + rect_width, top + rect_height);
		// 스키아 차례: 왼위, 오른위, 오른아래, 왼아래.
		let radius_array = [corner_list[0], corner_list[1], corner_list[3], corner_list[2]];
		match build_skia_round_rect(rect, radius_array) {
			SkiaRoundRect::Rect => {
				self.add_rect_points(matrix, rect, is_clockwise);
			}
			SkiaRoundRect::Oval => {
				self.add_oval_points(matrix, rect, is_clockwise);
			}
			SkiaRoundRect::RoundRect(radius_array) => {
				self.add_round_rect_points(matrix, rect, radius_array, is_clockwise);
			}
		}
		self.move_to_with(matrix, left, top);
		true
	}

	//==============================================================================
	// 닫힌 사각형 조각. (스키아 addRect, 시작 번호 0: 시계는 (L,T) (R,T) (R,B) (L,B), 반시계는 (L,T) (L,B) (R,B) (R,T))
	//==============================================================================
	fn add_rect_points(&mut self, matrix: &CanvasMatrix, rect: (f32, f32, f32, f32), is_clockwise: bool) {
		let (left, top, right, bottom) = rect;
		self.move_to_with(matrix, left, top);
		if is_clockwise {
			self.push_line(matrix, right, top);
			self.push_line(matrix, right, bottom);
			self.push_line(matrix, left, bottom);
		}
		else {
			self.push_line(matrix, left, bottom);
			self.push_line(matrix, right, bottom);
			self.push_line(matrix, right, top);
		}
		self.close_subpath();
	}

	//==============================================================================
	// 닫힌 타원 조각. (스키아 addOval, 시작 번호 0: 위 가운데에서 사분원 원뿔 곡선 넷)
	//==============================================================================
	fn add_oval_points(&mut self, matrix: &CanvasMatrix, rect: (f32, f32, f32, f32), is_clockwise: bool) {
		let (left, top, right, bottom) = rect;
		let center_x = ((left as f64 + right as f64) * 0.5) as f32;
		let center_y = ((top as f64 + bottom as f64) * 0.5) as f32;
		self.move_to_with(matrix, center_x, top);
		let step_list = if is_clockwise {
			[((right, top), (right, center_y)), ((right, bottom), (center_x, bottom)), ((left, bottom), (left, center_y)), ((left, top), (center_x, top))]
		}
		else {
			[((left, top), (left, center_y)), ((left, bottom), (center_x, bottom)), ((right, bottom), (right, center_y)), ((right, top), (center_x, top))]
		};
		for (control, end) in step_list {
			self.push_conic(matrix, control, end, SKIA_ROOT2_OVER2);
		}
		self.close_subpath();
	}

	//==============================================================================
	// 닫힌 둥근 사각형 조각. (스키아 set_as_rrect, 시작 번호 0. 시계는 곧은 선부터, 반시계는 왼위 모서리 곡선부터)
	//==============================================================================
	fn add_round_rect_points(&mut self, matrix: &CanvasMatrix, rect: (f32, f32, f32, f32), radius_array: [(f32, f32); 4], is_clockwise: bool) {
		let (left, top, right, bottom) = rect;
		let [upper_left, upper_right, lower_right, lower_left] = radius_array;
		let point_list = [
			(left + upper_left.0, top),
			(right - upper_right.0, top),
			(right, top + upper_right.1),
			(right, bottom - lower_right.1),
			(right - lower_right.0, bottom),
			(left + lower_left.0, bottom),
			(left, bottom - lower_left.1),
			(left, top + upper_left.1),
		];
		self.move_to_with(matrix, point_list[0].0, point_list[0].1);
		if is_clockwise {
			let corner_list = [(right, top), (right, bottom), (left, bottom), (left, top)];
			for step_index in 0..4 {
				let line_point = point_list[step_index * 2 + 1];
				let end_point = point_list[(step_index * 2 + 2) % 8];
				self.push_line(matrix, line_point.0, line_point.1);
				self.push_conic(matrix, corner_list[step_index], end_point, SKIA_ROOT2_OVER2);
			}
		}
		else {
			let corner_list = [(left, top), (left, bottom), (right, bottom), (right, top)];
			let conic_end_list = [point_list[7], point_list[5], point_list[3], point_list[1]];
			let line_list = [point_list[6], point_list[4], point_list[2]];
			for step_index in 0..4 {
				self.push_conic(matrix, corner_list[step_index], conic_end_list[step_index], SKIA_ROOT2_OVER2);
				if step_index < 3 {
					self.push_line(matrix, line_list[step_index].0, line_list[step_index].1);
				}
			}
		}
		self.close_subpath();
	}
}

//==============================================================================
// 시작 각을 [0, 2π) 로 다듬고 끝 각을 같은 만큼 옮긴다. (블링크 CanonicalizeAngle, 단정밀도 fmodf)
//==============================================================================
fn canonicalize_angle(start_angle: f32, end_angle: f32) -> (f32, f32) {
	let mut new_start_angle = start_angle % BLINK_TWO_PI_FLOAT;
	if new_start_angle < 0.0 {
		new_start_angle += BLINK_TWO_PI_FLOAT;
		if new_start_angle >= BLINK_TWO_PI_FLOAT {
			new_start_angle -= BLINK_TWO_PI_FLOAT;
		}
	}
	let delta = new_start_angle - start_angle;
	(new_start_angle, end_angle + delta)
}

//==============================================================================
// 끝 각 맞추기. (블링크 AdjustEndAngle: 그 방향으로 2π 이상이면 한 바퀴, 아니면 그 방향의 [0, 2π) 각)
//==============================================================================
fn adjust_end_angle(start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> f32 {
	if !is_counterclockwise && end_angle - start_angle >= BLINK_TWO_PI_FLOAT {
		start_angle + BLINK_TWO_PI_FLOAT
	}
	else if is_counterclockwise && start_angle - end_angle >= BLINK_TWO_PI_FLOAT {
		start_angle - BLINK_TWO_PI_FLOAT
	}
	else if !is_counterclockwise && start_angle > end_angle {
		start_angle + (BLINK_TWO_PI_FLOAT - (start_angle - end_angle) % BLINK_TWO_PI_FLOAT)
	}
	else if is_counterclockwise && start_angle < end_angle {
		start_angle - (BLINK_TWO_PI_FLOAT - (end_angle - start_angle) % BLINK_TWO_PI_FLOAT)
	}
	else {
		end_angle
	}
}

//==============================================================================
// 0 에 붙인 sin, cos. (스키아 SkScalarSinSnapToZero, SkScalarCosSnapToZero)
//==============================================================================
fn sin_snap_to_zero(radians: f32) -> f32 {
	let value = radians.sin();
	if value.abs() <= SKIA_SIN_COS_NEARLY_ZERO {
		0.0
	}
	else {
		value
	}
}

fn cos_snap_to_zero(radians: f32) -> f32 {
	let value = radians.cos();
	if value.abs() <= SKIA_SIN_COS_NEARLY_ZERO {
		0.0
	}
	else {
		value
	}
}

//==============================================================================
// 시작과 끝 단위 벡터, 시계 방향인지. (스키아 angles_to_unit_vectors, 각은 도. 359 ~ 360 도인데 두 벡터가 같으면 끝을 조금씩 당긴다)
//==============================================================================
fn convert_angles_to_unit_vectors(start_angle: f32, sweep_angle: f32) -> ((f32, f32), (f32, f32), bool) {
	let radians_per_degree = SKIA_FLOAT_PI / 180.0;
	let start_radians = start_angle * radians_per_degree;
	let mut stop_radians = (start_angle + sweep_angle) * radians_per_degree;
	let start_vector = (cos_snap_to_zero(start_radians), sin_snap_to_zero(start_radians));
	let mut stop_vector = (cos_snap_to_zero(stop_radians), sin_snap_to_zero(stop_radians));
	if start_vector == stop_vector {
		let sweep_size = sweep_angle.abs();
		if sweep_size < 360.0 && sweep_size > 359.0 {
			let delta_radians = (1.0f32 / 512.0).copysign(sweep_angle);
			while start_vector == stop_vector {
				stop_radians -= delta_radians;
				stop_vector = (cos_snap_to_zero(stop_radians), sin_snap_to_zero(stop_radians));
			}
		}
	}
	(start_vector, stop_vector, sweep_angle > 0.0)
}

//==============================================================================
// 벡터 길이 맞추기. (스키아 SkPoint::setLength: 배정밀도로 셈하고, 유한하지 않거나 0 이면 (0, 0))
//==============================================================================
fn set_vector_length(vector: (f32, f32), length: f32) -> (f32, f32) {
	let vector_x = vector.0 as f64;
	let vector_y = vector.1 as f64;
	let magnitude = (vector_x * vector_x + vector_y * vector_y).sqrt();
	let scale = length as f64 / magnitude;
	let scaled_x = (vector_x * scale) as f32;
	let scaled_y = (vector_y * scale) as f32;
	if !scaled_x.is_finite() || !scaled_y.is_finite() || (scaled_x == 0.0 && scaled_y == 0.0) {
		return (0.0, 0.0);
	}
	(scaled_x, scaled_y)
}

//==============================================================================
// 단위원 원호의 원뿔 곡선들. ((점 셋, 무게) 목록, 단위원 좌표. 스키아 SkConic::BuildUnitArc)
// - 사분원마다 무게 √2/2 의 곡선 하나, 남은 90 도 미만은 무게 cos(θ/2) 하나. 시작 벡터만큼 돌리고 반시계면 y 를 뒤집는다.
//==============================================================================
fn build_unit_arc(start_vector: (f32, f32), stop_vector: (f32, f32), is_clockwise: bool) -> Vec<([(f32, f32); 3], f32)> {
	let mut conic_list: Vec<([(f32, f32); 3], f32)> = Vec::new();
	let x = start_vector.0 * stop_vector.0 + start_vector.1 * stop_vector.1;
	let mut y = start_vector.0 * stop_vector.1 - start_vector.1 * stop_vector.0;
	let absolute_y = y.abs();
	if absolute_y <= SKIA_SCALAR_NEARLY_ZERO && x > 0.0 && ((y >= 0.0 && is_clockwise) || (y <= 0.0 && !is_clockwise)) {
		return conic_list;
	}
	if !is_clockwise {
		y = -y;
	}
	let quadrant = if y == 0.0 {
		2
	}
	else if x == 0.0 {
		if y > 0.0 {
			1
		}
		else {
			3
		}
	}
	else {
		let mut quadrant = 0;
		if y < 0.0 {
			quadrant += 2;
		}
		if (x < 0.0) != (y < 0.0) {
			quadrant += 1;
		}
		quadrant
	};
	let quadrant_point_list: [(f32, f32); 8] = [(1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0), (-1.0, -1.0), (0.0, -1.0), (1.0, -1.0)];
	for quadrant_index in 0..quadrant {
		conic_list.push(([quadrant_point_list[quadrant_index * 2], quadrant_point_list[quadrant_index * 2 + 1], quadrant_point_list[(quadrant_index * 2 + 2) % 8]], SKIA_ROOT2_OVER2));
	}
	// 남은 (90 도 미만) 원호.
	let final_point = (x, y);
	let last_quadrant_point = quadrant_point_list[quadrant * 2];
	let dot = last_quadrant_point.0 * final_point.0 + last_quadrant_point.1 * final_point.1;
	if dot.is_nan() {
		return Vec::new();
	}
	if dot < 1.0 {
		let cosine_half_theta = ((1.0 + dot) / 2.0).sqrt();
		let off_curve = set_vector_length((last_quadrant_point.0 + x, last_quadrant_point.1 + y), 1.0 / cosine_half_theta);
		let is_same = (last_quadrant_point.0 - off_curve.0).abs() <= SKIA_SCALAR_NEARLY_ZERO && (last_quadrant_point.1 - off_curve.1).abs() <= SKIA_SCALAR_NEARLY_ZERO;
		if !is_same {
			conic_list.push(([last_quadrant_point, off_curve, final_point], cosine_half_theta));
		}
	}
	// 시작 벡터만큼 돌리기. (반시계면 먼저 y 를 뒤집는다)
	let (sine, cosine) = (start_vector.1, start_vector.0);
	for (point_list, _) in conic_list.iter_mut() {
		for point in point_list.iter_mut() {
			let flipped_y = if is_clockwise {
				point.1
			}
			else {
				-point.1
			};
			*point = (cosine * point.0 - sine * flipped_y, sine * point.0 + cosine * flipped_y);
		}
	}
	conic_list
}

//==============================================================================
// 스키아 둥근 사각형의 꼴. (SkRRect::computeType 의 사각형, 타원, 그 밖. 그 밖은 다듬은 반지름 [왼위, 오른위, 오른아래, 왼아래])
//==============================================================================
enum SkiaRoundRect {
	Rect,
	Oval,
	RoundRect([(f32, f32); 4]),
}

//==============================================================================
// 반지름 다듬기. (스키아 SkRRect::setRectRadii: 한쪽이 0 이하인 모서리는 둘 다 0, 넘치면 모든 반지름을 한 비로 줄이고(배정밀도), 단정밀도 반올림으로 넘치면 한 ULP 씩 줄인다)
//==============================================================================
fn build_skia_round_rect(rect: (f32, f32, f32, f32), radius_array: [(f32, f32); 4]) -> SkiaRoundRect {
	let (left, top, right, bottom) = rect;
	let mut radius_array = radius_array;
	if radius_array.iter().any(|radius| !radius.0.is_finite() || !radius.1.is_finite()) {
		return SkiaRoundRect::Rect;
	}
	if clamp_radius_to_zero(&mut radius_array) {
		return SkiaRoundRect::Rect;
	}
	let was_simple = radius_array.iter().all(|radius| *radius == radius_array[0]);
	let was_circular = was_simple && radius_array[0].0 == radius_array[0].1;
	let width = right as f64 - left as f64;
	let height = bottom as f64 - top as f64;
	let compute_minimum_scale = |first_radius: f32, second_radius: f32, limit: f64, current_minimum: f64| -> f64 {
		let radius_sum = first_radius as f64 + second_radius as f64;
		if radius_sum > limit {
			current_minimum.min(limit / radius_sum)
		}
		else {
			current_minimum
		}
	};
	let mut scale = 1.0f64;
	scale = compute_minimum_scale(radius_array[0].0, radius_array[1].0, width, scale);
	scale = compute_minimum_scale(radius_array[1].1, radius_array[2].1, height, scale);
	scale = compute_minimum_scale(radius_array[2].0, radius_array[3].0, width, scale);
	scale = compute_minimum_scale(radius_array[3].1, radius_array[0].1, height, scale);
	let flush_to_zero = |first: &mut f32, second: &mut f32| {
		if *first + *second == *first {
			*second = 0.0;
		}
		else if *first + *second == *second {
			*first = 0.0;
		}
	};
	{
		let [upper_left, upper_right, lower_right, lower_left] = &mut radius_array;
		flush_to_zero(&mut upper_left.0, &mut upper_right.0);
		flush_to_zero(&mut upper_right.1, &mut lower_right.1);
		flush_to_zero(&mut lower_right.0, &mut lower_left.0);
		flush_to_zero(&mut lower_left.1, &mut upper_left.1);
		adjust_radii(scale, &mut upper_left.0, &mut upper_right.0, left, right);
		adjust_radii(scale, &mut upper_right.1, &mut lower_right.1, top, bottom);
		adjust_radii(scale, &mut lower_left.0, &mut lower_right.0, left, right);
		adjust_radii(scale, &mut upper_left.1, &mut lower_left.1, top, bottom);
	}
	if !clamp_radius_to_zero(&mut radius_array) && was_simple {
		let mut minimum_x = radius_array.iter().map(|radius| radius.0).fold(f32::INFINITY, f32::min);
		let mut minimum_y = radius_array.iter().map(|radius| radius.1).fold(f32::INFINITY, f32::min);
		if was_circular {
			minimum_x = minimum_x.min(minimum_y);
			minimum_y = minimum_x;
		}
		radius_array = [(minimum_x, minimum_y); 4];
	}
	// 꼴 정하기. (SkRRect::computeType)
	let is_all_square = radius_array.iter().all(|radius| radius.0 == 0.0 || radius.1 == 0.0);
	if is_all_square {
		return SkiaRoundRect::Rect;
	}
	let is_all_equal = radius_array.iter().all(|radius| *radius == radius_array[0]);
	let half_width = ((right as f64 - left as f64) * 0.5) as f32;
	let half_height = ((bottom as f64 - top as f64) * 0.5) as f32;
	if is_all_equal && radius_array[0].0 >= half_width && radius_array[0].1 >= half_height {
		return SkiaRoundRect::Oval;
	}
	SkiaRoundRect::RoundRect(radius_array)
}

//==============================================================================
// 한쪽이 0 이하인 모서리는 둘 다 0. (스키아 clamp_to_zero, 모든 모서리가 각지면 true)
//==============================================================================
fn clamp_radius_to_zero(radius_array: &mut [(f32, f32); 4]) -> bool {
	let mut is_all_square = true;
	for radius in radius_array.iter_mut() {
		if radius.0 <= 0.0 || radius.1 <= 0.0 {
			*radius = (0.0, 0.0);
		}
		else {
			is_all_square = false;
		}
	}
	is_all_square
}

//==============================================================================
// 0 쪽으로 다음 단정밀도 수. (C nextafterf(x, 0))
//==============================================================================
fn step_toward_zero(value: f32) -> f32 {
	if value == 0.0 || !value.is_finite() {
		return value;
	}
	f32::from_bits(value.to_bits() - 1)
}

//==============================================================================
// 한 변의 두 반지름 줄이기. (스키아 SkScaleToSides::AdjustRadii)
//==============================================================================
fn adjust_radii(scale: f64, first: &mut f32, second: &mut f32, minimum_coordinate: f32, maximum_coordinate: f32) {
	let limit = (maximum_coordinate as f64 - minimum_coordinate as f64).min((maximum_coordinate - minimum_coordinate) as f64);
	if scale < 1.0 {
		*first = (*first as f64 * scale) as f32;
		*second = (*second as f64 * scale) as f32;
	}
	if maximum_coordinate - *second < minimum_coordinate {
		*second = step_toward_zero(maximum_coordinate - minimum_coordinate);
	}
	if minimum_coordinate + *first > maximum_coordinate {
		*first = step_toward_zero(maximum_coordinate - minimum_coordinate);
	}
	if (*first + *second) as f64 > limit {
		let is_first_smaller = *first <= *second;
		let (minimum_radius, maximum_radius) = if is_first_smaller {
			(*first, &mut *second)
		}
		else {
			(*second, &mut *first)
		};
		let mut new_maximum_radius = (limit - minimum_radius as f64) as f32;
		while (new_maximum_radius + minimum_radius) as f64 > limit && new_maximum_radius > 0.0 {
			new_maximum_radius = step_toward_zero(new_maximum_radius);
		}
		*maximum_radius = new_maximum_radius;
	}
}

//==============================================================================
// 원뿔 곡선 → 2 차 곡선들. (스키아 SkAutoConicToQuads, 돌려주는 점: [곡선마다 조정점, 끝 점] 차례. 끝 점은 다음 곡선의 시작)
// - Floater(1993)의 오차로 조각 수(2 의 거듭제곱, 32 까지)를 정하고, 크롬이 켠 옛 판(SK_SUPPORT_LEGACY_CONIC_CHOP) chop 으로 반씩 나눈다.
// - 시작과 끝이 y 로 단조로우면 나눈 점도 단조롭게 고친다. 가장 많이 나눌 때 첫 나눔이 선 둘이면 선으로 갈음한다.
// - tolerance 는 원뿔 곡선 좌표계의 길이다. (스키아 래스터는 기기 픽셀 0.25)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn convert_conic_to_quads(start: (f32, f32), control: (f32, f32), end: (f32, f32), weight: f32, tolerance: f32) -> Vec<(f32, f32)> {
	let is_bad_weight = weight < 0.0 || !weight.is_finite();
	let is_finite = [start, control, end].iter().all(|point| point.0.is_finite() && point.1.is_finite());
	// 2 의 거듭제곱 정하기. (SkConic::computeQuadPOW2)
	let mut power = 0;
	if tolerance >= 0.0 && tolerance.is_finite() && is_finite && !is_bad_weight {
		let a = weight - 1.0;
		let k = a / (4.0 * (2.0 + a));
		let error_x = k * (start.0 - 2.0 * control.0 + end.0);
		let error_y = k * (start.1 - 2.0 * control.1 + end.1);
		let mut error = (error_x * error_x + error_y * error_y).sqrt();
		while power < SKIA_MAXIMUM_CONIC_TO_QUAD_POWER {
			if error <= tolerance {
				break;
			}
			error *= 0.25;
			power += 1;
		}
	}
	if is_bad_weight {
		power = 0;
	}
	let mut point_list: Vec<(f32, f32)> = Vec::with_capacity((2usize << power) + 1);
	point_list.push(start);
	let conic = ([start, control, end], weight);
	let mut is_line_pair = false;
	if power == SKIA_MAXIMUM_CONIC_TO_QUAD_POWER {
		let (first_conic, second_conic) = chop_conic(&conic);
		let is_near = |first: (f32, f32), second: (f32, f32)| -> bool {
			(first.0 - second.0).abs() <= SKIA_SCALAR_NEARLY_ZERO && (first.1 - second.1).abs() <= SKIA_SCALAR_NEARLY_ZERO
		};
		if is_near(first_conic.0[1], first_conic.0[2]) && is_near(second_conic.0[0], second_conic.0[1]) {
			point_list.extend([first_conic.0[1], first_conic.0[1], first_conic.0[1], second_conic.0[2]]);
			is_line_pair = true;
		}
	}
	if !is_line_pair {
		subdivide_conic(&conic, &mut point_list, power);
	}
	if point_list.iter().any(|point| !point.0.is_finite() || !point.1.is_finite()) {
		let point_count = point_list.len();
		for point in point_list.iter_mut().take(point_count - 1).skip(1) {
			*point = control;
		}
	}
	if weight <= 0.0 {
		return vec![start, ((start.0 + end.0) * 0.5, (start.1 + end.1) * 0.5), end];
	}
	point_list
}

//==============================================================================
// 원뿔 곡선 반으로 나누기. (스키아 SkConic::chop 옛 판: 동차 좌표로 가운데를 셈하고 무게는 sqrt(1/2 + w/2))
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn chop_conic(conic: &([(f32, f32); 3], f32)) -> (([(f32, f32); 3], f32), ([(f32, f32); 3], f32)) {
	let ([first, second, third], weight) = *conic;
	let scale = 1.0 / (1.0 + weight);
	let new_weight = (0.5 + weight * 0.5).sqrt();
	let weighted_second = (weight * second.0, weight * second.1);
	let mut middle = ((first.0 + (weighted_second.0 + weighted_second.0) + third.0) * scale * 0.5, (first.1 + (weighted_second.1 + weighted_second.1) + third.1) * scale * 0.5);
	if !middle.0.is_finite() || !middle.1.is_finite() {
		let weight_double = weight as f64;
		let scale_half = 1.0 / (1.0 + weight_double) * 0.5;
		middle = (((first.0 as f64 + weight_double * 2.0 * second.0 as f64 + third.0 as f64) * scale_half) as f32, ((first.1 as f64 + weight_double * 2.0 * second.1 as f64 + third.1 as f64) * scale_half) as f32);
	}
	let first_control = ((first.0 + weighted_second.0) * scale, (first.1 + weighted_second.1) * scale);
	let second_control = ((weighted_second.0 + third.0) * scale, (weighted_second.1 + third.1) * scale);
	(([first, first_control, middle], new_weight), ([middle, second_control, third], new_weight))
}

//==============================================================================
// 원뿔 곡선을 level 번 반씩 나눠 2 차 곡선 점 넣기. (스키아 subdivide, y 단조 고치기)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn subdivide_conic(conic: &([(f32, f32); 3], f32), point_list: &mut Vec<(f32, f32)>, level: u32) {
	if level == 0 {
		point_list.push(conic.0[1]);
		point_list.push(conic.0[2]);
		return;
	}
	let (mut first_conic, mut second_conic) = chop_conic(conic);
	let is_between = |a: f32, b: f32, c: f32| -> bool {
		(a - b) * (c - b) <= 0.0
	};
	let start_y = conic.0[0].1;
	let end_y = conic.0[2].1;
	if is_between(start_y, conic.0[1].1, end_y) {
		let middle_y = first_conic.0[2].1;
		if !is_between(start_y, middle_y, end_y) {
			let closer_y = if (middle_y - start_y).abs() < (middle_y - end_y).abs() {
				start_y
			}
			else {
				end_y
			};
			first_conic.0[2].1 = closer_y;
			second_conic.0[0].1 = closer_y;
		}
		if !is_between(start_y, first_conic.0[1].1, first_conic.0[2].1) {
			first_conic.0[1].1 = start_y;
		}
		if !is_between(second_conic.0[0].1, second_conic.0[1].1, end_y) {
			second_conic.0[1].1 = end_y;
		}
	}
	subdivide_conic(&first_conic, point_list, level - 1);
	subdivide_conic(&second_conic, point_list, level - 1);
}

//==============================================================================
// 원뿔 곡선 위 점. (유리 2 차 곡선, t 는 0 ~ 1)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn evaluate_conic(start: (f32, f32), control: (f32, f32), end: (f32, f32), weight: f32, time: f32) -> (f32, f32) {
	let inverse_time = 1.0 - time;
	let start_factor = inverse_time * inverse_time;
	let control_factor = 2.0 * weight * time * inverse_time;
	let end_factor = time * time;
	let denominator = start_factor + control_factor + end_factor;
	((start_factor * start.0 + control_factor * control.0 + end_factor * end.0) / denominator, (start_factor * start.1 + control_factor * control.1 + end_factor * end.1) / denominator)
}

//==============================================================================
// 그라데이션 종류.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasGradientKind {
	// 원본 createLinearGradient(x0, y0, x1, y1)
	Linear {
		start_x: f32,
		start_y: f32,
		end_x: f32,
		end_y: f32,
	},
	// 원본 createRadialGradient(x0, y0, r0, x1, y1, r1)
	Radial {
		start_x: f32,
		start_y: f32,
		start_radius: f32,
		end_x: f32,
		end_y: f32,
		end_radius: f32,
	},
	// 원본 createConicGradient(startAngle, x, y)
	Conic {
		start_angle: f32,
		center_x: f32,
		center_y: f32,
	},
}

//==============================================================================
// 그라데이션. (원본 CanvasGradient)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasGradient {
	kind: CanvasGradientKind,
	stop_list: Vec<(f32, Color)>,
}

impl CanvasGradient {
	//==============================================================================
	// 만들기. (캔버스의 create_*_gradient 가 부른다)
	//==============================================================================
	fn new(kind: CanvasGradientKind) -> CanvasGradient {
		CanvasGradient {
			kind,
			stop_list: Vec::new(),
		}
	}

	//==============================================================================
	// 색 멈춤 더하기. (원본 addColorStop, 자리가 0 ~ 1 밖이면 원본의 예외처럼 더하지 않고 false. 같은 자리는 넣은 차례대로)
	//==============================================================================
	pub fn add_color_stop(&mut self, offset: f32, color: impl Into<Color>) -> bool {
		if !(0.0..=1.0).contains(&offset) {
			return false;
		}
		let color = color.into();
		let insert_index = self.stop_list.iter().position(|(stop_offset, _)| *stop_offset > offset).unwrap_or(self.stop_list.len());
		self.stop_list.insert(insert_index, (offset, color));
		true
	}

	//==============================================================================
	// 종류.
	//==============================================================================
	pub fn get_kind(&self) -> CanvasGradientKind {
		self.kind
	}

	//==============================================================================
	// 색 멈춤 목록. (자리 차례)
	//==============================================================================
	pub fn get_color_stops(&self) -> &[(f32, Color)] {
		&self.stop_list
	}
}

//==============================================================================
// 패턴 되풀이. (원본 "repeat", "repeat-x", "repeat-y", "no-repeat")
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasPatternRepetition {
	Repeat,
	RepeatX,
	RepeatY,
	NoRepeat,
}

impl CanvasPatternRepetition {
	//==============================================================================
	// 원본 글.
	//==============================================================================
	pub fn get_name(&self) -> &'static str {
		match self {
			CanvasPatternRepetition::Repeat => {
				"repeat"
			}
			CanvasPatternRepetition::RepeatX => {
				"repeat-x"
			}
			CanvasPatternRepetition::RepeatY => {
				"repeat-y"
			}
			CanvasPatternRepetition::NoRepeat => {
				"no-repeat"
			}
		}
	}
}

//==============================================================================
// 패턴. (원본 CanvasPattern, 만들 때의 그림을 프리멀티플라이드 RGBA 로 든다)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasPattern {
	width: u32,
	height: u32,
	pixels: Vec<u8>,
	repetition: CanvasPatternRepetition,
	transform: CanvasMatrix,
}

impl CanvasPattern {
	//==============================================================================
	// 패턴 변환 설정. (원본 setTransform)
	//==============================================================================
	pub fn set_transform(&mut self, transform: CanvasMatrix) {
		if transform.is_finite() {
			self.transform = transform;
		}
	}

	//==============================================================================
	// 되풀이.
	//==============================================================================
	pub fn get_repetition(&self) -> CanvasPatternRepetition {
		self.repetition
	}
}

//==============================================================================
// 칠할 것. (원본 fillStyle, strokeStyle 의 색 글, CanvasGradient, CanvasPattern)
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub enum CanvasStyle {
	Color(Color),
	Gradient(CanvasGradient),
	Pattern(CanvasPattern),
}

impl From<Color> for CanvasStyle {
	fn from(color: Color) -> CanvasStyle {
		CanvasStyle::Color(color)
	}
}

impl From<&str> for CanvasStyle {
	fn from(color_string: &str) -> CanvasStyle {
		CanvasStyle::Color(Color::from(color_string))
	}
}

impl From<&String> for CanvasStyle {
	fn from(color_string: &String) -> CanvasStyle {
		CanvasStyle::Color(Color::from(color_string.as_str()))
	}
}

impl From<CanvasGradient> for CanvasStyle {
	fn from(gradient: CanvasGradient) -> CanvasStyle {
		CanvasStyle::Gradient(gradient)
	}
}

impl From<&CanvasGradient> for CanvasStyle {
	fn from(gradient: &CanvasGradient) -> CanvasStyle {
		CanvasStyle::Gradient(gradient.clone())
	}
}

impl From<CanvasPattern> for CanvasStyle {
	fn from(pattern: CanvasPattern) -> CanvasStyle {
		CanvasStyle::Pattern(pattern)
	}
}

impl From<&CanvasPattern> for CanvasStyle {
	fn from(pattern: &CanvasPattern) -> CanvasStyle {
		CanvasStyle::Pattern(pattern.clone())
	}
}

//==============================================================================
// 채우기 규칙. (원본 "nonzero", "evenodd")
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasFillRule {
	#[default]
	NonZero,
	EvenOdd,
}

//==============================================================================
// 선 끝. (원본 lineCap)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasLineCap {
	#[default]
	Butt,
	Round,
	Square,
}

//==============================================================================
// 선 이음. (원본 lineJoin)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasLineJoin {
	#[default]
	Miter,
	Round,
	Bevel,
}

//==============================================================================
// 글 가로 정렬. (원본 textAlign, start 와 end 는 왼쪽에서 오른쪽 글로 본다)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasTextAlign {
	#[default]
	Start,
	End,
	Left,
	Right,
	Center,
}

//==============================================================================
// 글 기준선. (원본 textBaseline)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CanvasTextBaseline {
	Top,
	Hanging,
	Middle,
	#[default]
	Alphabetic,
	Ideographic,
	Bottom,
}

//==============================================================================
// 픽셀 묶음. (원본 ImageData, 프리멀티플라이하지 않은 RGBA)
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
	pub width: u32,
	pub height: u32,
	pub data: Vec<u8>,
}

impl ImageData {
	//==============================================================================
	// 투명한 검정으로 채운 픽셀 묶음. (원본 createImageData, new ImageData(w, h))
	//==============================================================================
	pub fn new(width: u32, height: u32) -> ImageData {
		ImageData {
			width,
			height,
			data: vec![0; width as usize * height as usize * 4],
		}
	}
}

//==============================================================================
// 그릴 그림. (원본 drawImage 와 createPattern 이 받는 그림: 엔진 Image, 다른 캔버스)
//==============================================================================
#[derive(Clone, Copy)]
pub enum CanvasImageSource<'a> {
	Image(&'a crate::core::image::Image),
	Canvas(&'a Canvas2D),
}

impl<'a> From<&'a crate::core::image::Image> for CanvasImageSource<'a> {
	fn from(image: &'a crate::core::image::Image) -> CanvasImageSource<'a> {
		CanvasImageSource::Image(image)
	}
}

impl<'a> From<&'a std::rc::Rc<crate::core::image::Image>> for CanvasImageSource<'a> {
	fn from(image: &'a std::rc::Rc<crate::core::image::Image>) -> CanvasImageSource<'a> {
		CanvasImageSource::Image(image.as_ref())
	}
}

impl<'a> From<&'a Canvas2D> for CanvasImageSource<'a> {
	fn from(canvas: &'a Canvas2D) -> CanvasImageSource<'a> {
		CanvasImageSource::Canvas(canvas)
	}
}

impl CanvasImageSource<'_> {
	//==============================================================================
	// 크기.
	//==============================================================================
	pub fn get_size(&self) -> (u32, u32) {
		match self {
			CanvasImageSource::Image(image) => {
				(image.get_width(), image.get_height())
			}
			CanvasImageSource::Canvas(canvas) => {
				(canvas.get_width(), canvas.get_height())
			}
		}
	}

	//==============================================================================
	// 프리멀티플라이드 RGBA 픽셀. (엔진 Image 가 GPU 에 올린 뒤 픽셀을 놓았으면 None)
	//==============================================================================
	pub(crate) fn read_premultiplied_pixels(&self) -> Option<Vec<u8>> {
		match self {
			CanvasImageSource::Image(image) => {
				// GPU 에 올리며 놓은 그림은 텍스처에서 되읽는다. (크롬이 GPU 에 있는 그림을 CPU 캔버스에 그릴 때와 같다)
				if !crate::core::imagetexturecache::restore_image_pixels_from_texture(image) {
					eprintln!("Canvas2D: 그림의 CPU 픽셀이 없고 텍스처에서도 되읽지 못해 그리지 못합니다.");
					return None;
				}
				Some(image.get_pixels().to_vec())
			}
			CanvasImageSource::Canvas(canvas) => {
				Some(canvas.read_premultiplied_pixels())
			}
		}
	}
}

//==============================================================================
// 프리멀티플라이드 → 곧은 RGBA. (원본 getImageData 처럼 반올림)
//==============================================================================
pub(crate) fn unpremultiply_pixels(premultiplied_pixels: &[u8]) -> Vec<u8> {
	let mut straight_pixels = premultiplied_pixels.to_vec();
	for pixel in straight_pixels.chunks_exact_mut(4) {
		let alpha = pixel[3] as u32;
		if alpha == 0 {
			pixel[0] = 0;
			pixel[1] = 0;
			pixel[2] = 0;
		}
		else if alpha < 255 {
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
			}
		}
	}
	straight_pixels
}

//==============================================================================
// 곧은 → 프리멀티플라이드 RGBA.
//==============================================================================
pub(crate) fn premultiply_pixels(straight_pixels: &[u8]) -> Vec<u8> {
	let mut premultiplied_pixels = straight_pixels.to_vec();
	for pixel in premultiplied_pixels.chunks_exact_mut(4) {
		let alpha = pixel[3] as u32;
		if alpha < 255 {
			for channel_index in 0..3 {
				pixel[channel_index] = ((pixel[channel_index] as u32 * alpha + 127) / 255) as u8;
			}
		}
	}
	premultiplied_pixels
}

//==============================================================================
// 합치기 이름 → 알아듣는 이름인지. (원본 globalCompositeOperation 이 받는 값, 모르는 값은 원본처럼 무시한다)
//==============================================================================
pub(crate) fn is_composite_operation_name(name: &str) -> bool {
	matches!(name, "source-over" | "source-in" | "source-out" | "source-atop" | "destination-over" | "destination-in" | "destination-out" | "destination-atop" | "lighter" | "copy" | "xor" | "multiply" | "screen" | "overlay" | "darken" | "lighten" | "color-dodge" | "color-burn" | "hard-light" | "soft-light" | "difference" | "exclusion" | "hue" | "saturation" | "color" | "luminosity")
}

//==============================================================================
// 선 점선 목록이 받을 수 있는지와 짝 맞추기. (원본 setLineDash: 음수나 유한하지 않은 값이 있으면 무시, 홀수 개면 두 번 잇는다)
//==============================================================================
pub(crate) fn normalize_line_dash(dash_list: &[f32]) -> Option<Vec<f32>> {
	if dash_list.iter().any(|dash| !dash.is_finite() || *dash < 0.0) {
		return None;
	}
	let mut normalized_list = dash_list.to_vec();
	if normalized_list.len() % 2 == 1 {
		normalized_list.extend_from_slice(dash_list);
	}
	Some(normalized_list)
}

#[cfg(not(target_arch = "wasm32"))]
mod desktop_canvas;
#[cfg(not(target_arch = "wasm32"))]
pub use desktop_canvas::Canvas2D;
#[cfg(target_arch = "wasm32")]
mod web_canvas;
#[cfg(target_arch = "wasm32")]
pub use web_canvas::Canvas2D;

//==============================================================================
// 시험. (경로 셈, 데스크탑)
//==============================================================================
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
	use super::*;

	// 조각이 기대와 가까운지. (단정밀도 반올림만큼)
	fn assert_segments_near(actual: &[PathSegment], expected: &[PathSegment]) {
		assert_eq!(actual.len(), expected.len(), "{:?}", actual);
		let flatten = |segment: &PathSegment| -> Vec<f32> {
			match *segment {
				PathSegment::MoveTo(x, y) | PathSegment::LineTo(x, y) => {
					vec![x, y]
				}
				PathSegment::QuadraticTo(a, b, c, d) => {
					vec![a, b, c, d]
				}
				PathSegment::CubicTo(a, b, c, d, e, f) => {
					vec![a, b, c, d, e, f]
				}
				PathSegment::ConicTo(a, b, c, d, w) => {
					vec![a, b, c, d, w]
				}
				PathSegment::Close => {
					Vec::new()
				}
			}
		};
		for (actual_segment, expected_segment) in actual.iter().zip(expected.iter()) {
			assert_eq!(std::mem::discriminant(actual_segment), std::mem::discriminant(expected_segment), "{:?}", actual);
			for (actual_value, expected_value) in flatten(actual_segment).iter().zip(flatten(expected_segment).iter()) {
				assert!((actual_value - expected_value).abs() < 1e-3, "{:?} != {:?}", actual, expected);
			}
		}
	}

	//==============================================================================
	// arc: 스키아처럼 사분원마다 무게 √2/2 의 원뿔 곡선, 한 바퀴는 반 바퀴 둘, 남은 각은 무게 cos(θ/2). 반지름 0 은 시작 점까지 곧은 선.
	//==============================================================================
	#[test]
	fn arc_builds_skia_conics() {
		let weight = std::f32::consts::FRAC_1_SQRT_2;
		let mut quarter_path = Path2D::new();
		assert!(quarter_path.arc(0.0, 0.0, 10.0, 0.0, PI / 2.0, false));
		assert_segments_near(quarter_path.get_segment_list(), &[PathSegment::MoveTo(10.0, 0.0), PathSegment::ConicTo(10.0, 10.0, 0.0, 10.0, weight)]);

		let mut circle_path = Path2D::new();
		assert!(circle_path.arc(0.0, 0.0, 10.0, 0.0, PI * 2.0, false));
		assert_segments_near(circle_path.get_segment_list(), &[
			PathSegment::MoveTo(10.0, 0.0),
			PathSegment::ConicTo(10.0, 10.0, 0.0, 10.0, weight),
			PathSegment::ConicTo(-10.0, 10.0, -10.0, 0.0, weight),
			PathSegment::ConicTo(-10.0, -10.0, 0.0, -10.0, weight),
			PathSegment::ConicTo(10.0, -10.0, 10.0, 0.0, weight),
		]);

		// 반시계로 0 에서 60 도는 300 도: 사분원 셋과 남은 30 도의 무게 cos(15 도).
		let mut sixty_path = Path2D::new();
		assert!(sixty_path.arc(0.0, 0.0, 10.0, 0.0, PI / 3.0, true));
		let segment_list = sixty_path.get_segment_list();
		assert_eq!(segment_list.len(), 5);
		if let PathSegment::ConicTo(_, _, end_x, end_y, last_weight) = segment_list[4] {
			assert!((end_x - 5.0).abs() < 1e-3 && (end_y - 8.660254).abs() < 1e-3);
			assert!((last_weight - (PI / 12.0).cos()).abs() < 1e-5);
		}
		else {
			panic!("{:?}", segment_list);
		}

		let mut zero_path = Path2D::new();
		zero_path.move_to(1.0, 1.0);
		assert!(zero_path.arc(5.0, 5.0, 0.0, 0.0, PI, false));
		assert_segments_near(zero_path.get_segment_list(), &[PathSegment::MoveTo(1.0, 1.0), PathSegment::LineTo(5.0, 5.0)]);
		assert!(!zero_path.arc(5.0, 5.0, -1.0, 0.0, PI, false));
	}

	//==============================================================================
	// arcTo: 첫 접점까지 곧은 선과 무게 sqrt((1 + cos) / 2) 의 원뿔 곡선. ellipse 의 0 반지름은 끝점을 잇는 곧은 선.
	//==============================================================================
	#[test]
	fn arc_to_and_degenerate_ellipse() {
		let mut path = Path2D::new();
		path.move_to(0.0, 0.0);
		assert!(path.arc_to(10.0, 0.0, 10.0, 10.0, 5.0));
		assert_segments_near(path.get_segment_list(), &[PathSegment::MoveTo(0.0, 0.0), PathSegment::LineTo(5.0, 0.0), PathSegment::ConicTo(10.0, 0.0, 10.0, 5.0, std::f32::consts::FRAC_1_SQRT_2)]);

		let mut flat_path = Path2D::new();
		assert!(flat_path.ellipse(0.0, 0.0, 10.0, 0.0, 0.0, 0.0, PI, false));
		// 시작 점(lineTo 라 moveTo 와 같은 점의 lineTo), 0.5π 의 (0, 0), 끝 점 (-10, 0).
		assert_segments_near(flat_path.get_segment_list(), &[PathSegment::MoveTo(10.0, 0.0), PathSegment::LineTo(10.0, 0.0), PathSegment::LineTo(0.0, 0.0), PathSegment::LineTo(-10.0, 0.0)]);
	}

	//==============================================================================
	// roundRect: 타원 모서리(DOMPointInit), 넘치면 x 와 y 를 같은 비로 줄이기, 모두 반이면 타원, 음수 폭은 반시계와 뒤집은 시작 점.
	//==============================================================================
	#[test]
	fn round_rect_follows_blink_and_skia() {
		let weight = std::f32::consts::FRAC_1_SQRT_2;
		let mut path = Path2D::new();
		assert!(path.round_rect(0.0, 0.0, 100.0, 50.0, &[(20.0, 10.0)]));
		assert_segments_near(path.get_segment_list(), &[
			PathSegment::MoveTo(20.0, 0.0),
			PathSegment::LineTo(80.0, 0.0),
			PathSegment::ConicTo(100.0, 0.0, 100.0, 10.0, weight),
			PathSegment::LineTo(100.0, 40.0),
			PathSegment::ConicTo(100.0, 50.0, 80.0, 50.0, weight),
			PathSegment::LineTo(20.0, 50.0),
			PathSegment::ConicTo(0.0, 50.0, 0.0, 40.0, weight),
			PathSegment::LineTo(0.0, 10.0),
			PathSegment::ConicTo(0.0, 0.0, 20.0, 0.0, weight),
			PathSegment::Close,
			PathSegment::MoveTo(0.0, 0.0),
		]);

		// 반지름 (80, 40) 은 넘쳐서 둘 다 0.625 배(폭 100 / (80 + 80)) 라 (50, 25), 곧 반이라 스키아는 타원으로 바꾼다.
		let mut scaled_path = Path2D::new();
		assert!(scaled_path.round_rect(0.0, 0.0, 100.0, 50.0, &[CanvasRadius::Elliptical { x: 80.0, y: 40.0 }]));
		assert_segments_near(&scaled_path.get_segment_list()[0..2], &[PathSegment::MoveTo(50.0, 0.0), PathSegment::ConicTo(100.0, 0.0, 100.0, 25.0, weight)]);
		// 반지름 (60, 10): x 만 넘쳐도 0.8333 배를 y 에도 곱한다. (50, 8.333)
		let mut uneven_path = Path2D::new();
		assert!(uneven_path.round_rect(0.0, 0.0, 100.0, 50.0, &[CanvasRadius::Elliptical { x: 60.0, y: 10.0 }]));
		assert_segments_near(&uneven_path.get_segment_list()[0..3], &[PathSegment::MoveTo(50.0, 0.0), PathSegment::LineTo(50.0, 0.0), PathSegment::ConicTo(100.0, 0.0, 100.0, 8.333333, weight)]);

		// 모든 반지름이 반이면 타원: 위 가운데에서 시작해 원뿔 곡선 넷.
		let mut oval_path = Path2D::new();
		assert!(oval_path.round_rect(0.0, 0.0, 40.0, 40.0, &[20.0]));
		assert_segments_near(&oval_path.get_segment_list()[0..2], &[PathSegment::MoveTo(20.0, 0.0), PathSegment::ConicTo(40.0, 0.0, 40.0, 20.0, weight)]);

		// 음수 폭: 왼쪽을 x + 폭으로 옮기고 반시계, 끝의 새 조각도 뒤집은 자리.
		let mut flipped_path = Path2D::new();
		assert!(flipped_path.round_rect(100.0, 0.0, -100.0, 50.0, &[10.0, 0.0, 0.0, 0.0]));
		let segment_list = flipped_path.get_segment_list();
		assert_segments_near(&segment_list[0..2], &[PathSegment::MoveTo(0.0, 0.0), PathSegment::ConicTo(0.0, 0.0, 0.0, 0.0, weight)]);
		assert_segments_near(&segment_list[segment_list.len() - 1..], &[PathSegment::MoveTo(0.0, 0.0)]);

		// 틀린 반지름.
		assert!(!path.round_rect(0.0, 0.0, 10.0, 10.0, &[-1.0]));
		assert!(!path.round_rect::<f32>(0.0, 0.0, 10.0, 10.0, &[]));
		assert!(!path.round_rect(0.0, 0.0, 10.0, 10.0, &[CanvasRadius::Elliptical { x: 1.0, y: -1.0 }]));
	}

	//==============================================================================
	// 원뿔 곡선 → 2 차 곡선: 반지름 100 의 사분원은 오차 셈으로 8 조각, 끝점은 모두 원 위.
	//==============================================================================
	#[test]
	fn conic_to_quads_like_skia() {
		let point_list = convert_conic_to_quads((100.0, 0.0), (100.0, 100.0), (0.0, 100.0), std::f32::consts::FRAC_1_SQRT_2, 0.25);
		assert_eq!(point_list.len(), 17);
		for point in point_list.iter().step_by(2) {
			let radius = (point.0 * point.0 + point.1 * point.1).sqrt();
			assert!((radius - 100.0).abs() < 0.01, "{:?}", point);
		}
		// 허용 오차가 크면 한 조각.
		assert_eq!(convert_conic_to_quads((100.0, 0.0), (100.0, 100.0), (0.0, 100.0), std::f32::consts::FRAC_1_SQRT_2, 10.0).len(), 3);
		let middle = evaluate_conic((100.0, 0.0), (100.0, 100.0), (0.0, 100.0), std::f32::consts::FRAC_1_SQRT_2, 0.5);
		assert!(((middle.0 * middle.0 + middle.1 * middle.1).sqrt() - 100.0).abs() < 0.01);
	}
}
