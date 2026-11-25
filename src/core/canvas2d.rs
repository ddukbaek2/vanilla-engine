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
//   그림자는 모양을 따로 칠해 흐리고(시그마 = shadowBlur / 2, 상자 흐림 세 번) 변환을 받지 않는 shadowOffset 만큼 옮겨 깐다.
//   copy, source-in, source-out, destination-in, destination-atop 은 원본처럼 모양 밖도 지운다. (모양을 따로 칠해 자른 영역 전체에 합친다)
//   글은 엔진 글자 캐시(Text 컴포넌트와 같은 얼굴 고르기, 모양 짓기, 대체 글꼴, 색 이모지)로 덮인 정도를 받아 칠한다.
// - 원본과 다른 점:
//   - 속성은 setter 와 getter 다. 색은 Color 나 CSS 색 글, 그라데이션, 패턴을 받는 CanvasStyle 이다.
//   - 경로의 arc, ellipse, arcTo, roundRect 는 원본(Skia 의 원뿔 곡선)과 달리 3 차 베지어로 잇는다. (90 도마다 한 조각, 반지름의 0.03% 안쪽)
//   - roundRect 의 모서리는 수 반지름만 받는다. (원본은 DOMPointInit 로 타원 모서리도 받는다)
//   - 데스크탑에서 엔진 Image 를 그리려면 그 그림의 CPU 픽셀이 남아 있어야 한다. (GPU 에 올린 뒤 놓았으면 그리지 못하고 알린다, Image::set_pixels_retained)
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
// 경로 조각. (점은 경로가 든 좌표계, 원본의 arc 와 roundRect 는 3 차 베지어로 풀어 둔다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PathSegment {
	MoveTo(f32, f32),
	LineTo(f32, f32),
	QuadraticTo(f32, f32, f32, f32),
	CubicTo(f32, f32, f32, f32, f32, f32),
	Close,
}

//==============================================================================
// 경로. (원본 Path2D, 캔버스의 지금 경로도 이것이다)
// - 더하는 함수의 점은 matrix 로 옮겨 담는다. Path2D 는 단위 행렬이고, 캔버스의 지금 경로는 더할 때의 변환이다. (원본과 같다)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path2D {
	segment_list: Vec<PathSegment>,
	// 마지막 점과 지금 조각의 시작 점. (담은 좌표계, 조각이 없으면 None)
	current_point: Option<(f32, f32)>,
	subpath_start_point: Option<(f32, f32)>,
}

impl Path2D {
	//==============================================================================
	// 빈 경로.
	//==============================================================================
	pub fn new() -> Path2D {
		Path2D::default()
	}

	//==============================================================================
	// 조각 목록.
	//==============================================================================
	pub(crate) fn get_segment_list(&self) -> &[PathSegment] {
		&self.segment_list
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
	}

	//==============================================================================
	// 곧은 선. (원본 lineTo, 조각이 없으면 그 점에서 조각을 시작한다)
	//==============================================================================
	pub fn line_to(&mut self, x: f32, y: f32) {
		self.line_to_with(&CanvasMatrix::IDENTITY, x, y);
	}

	//==============================================================================
	// 2 차 곡선. (원본 quadraticCurveTo)
	//==============================================================================
	pub fn quadratic_curve_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
		self.quadratic_curve_to_with(&CanvasMatrix::IDENTITY, control_x, control_y, x, y);
	}

	//==============================================================================
	// 3 차 곡선. (원본 bezierCurveTo)
	//==============================================================================
	pub fn bezier_curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, x: f32, y: f32) {
		self.bezier_curve_to_with(&CanvasMatrix::IDENTITY, first_control_x, first_control_y, second_control_x, second_control_y, x, y);
	}

	//==============================================================================
	// 원호. (원본 arc, 반지름이 음수면 원본의 예외처럼 아무것도 하지 않고 false)
	//==============================================================================
	pub fn arc(&mut self, x: f32, y: f32, radius: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		self.ellipse_with(&CanvasMatrix::IDENTITY, x, y, radius, radius, 0.0, start_angle, end_angle, is_counterclockwise)
	}

	//==============================================================================
	// 두 접선 사이 원호. (원본 arcTo, 반지름이 음수면 false)
	//==============================================================================
	pub fn arc_to(&mut self, first_x: f32, first_y: f32, second_x: f32, second_y: f32, radius: f32) -> bool {
		self.arc_to_with(&CanvasMatrix::IDENTITY, first_x, first_y, second_x, second_y, radius)
	}

	//==============================================================================
	// 타원호. (원본 ellipse, 반지름이 음수면 false)
	//==============================================================================
	pub fn ellipse(&mut self, x: f32, y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		self.ellipse_with(&CanvasMatrix::IDENTITY, x, y, radius_x, radius_y, rotation, start_angle, end_angle, is_counterclockwise)
	}

	//==============================================================================
	// 사각형. (원본 rect, 닫힌 조각을 더하고 (x, y) 에서 새 조각)
	//==============================================================================
	pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
		self.rect_with(&CanvasMatrix::IDENTITY, x, y, width, height);
	}

	//==============================================================================
	// 둥근 사각형. (원본 roundRect, 반지름 1 ~ 4 개. 틀리면 false)
	//==============================================================================
	pub fn round_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius_list: &[f32]) -> bool {
		self.round_rect_with(&CanvasMatrix::IDENTITY, x, y, width, height, radius_list)
	}

	//==============================================================================
	// 조각 닫기. (원본 closePath)
	//==============================================================================
	pub fn close_path(&mut self) {
		if self.current_point.is_none() {
			return;
		}
		self.segment_list.push(PathSegment::Close);
		self.current_point = self.subpath_start_point;
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
				PathSegment::Close => {
					self.close_path();
				}
			}
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
	// rect. (옮겨 담는 행렬과 함께)
	//==============================================================================
	pub(crate) fn rect_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32, width: f32, height: f32) {
		if !(x.is_finite() && y.is_finite() && width.is_finite() && height.is_finite()) {
			return;
		}
		self.move_to_with(matrix, x, y);
		self.push_line(matrix, x + width, y);
		self.push_line(matrix, x + width, y + height);
		self.push_line(matrix, x, y + height);
		self.close_path();
		self.move_to_with(matrix, x, y);
	}

	//==============================================================================
	// ellipse. (옮겨 담는 행렬과 함께. 조각이 있으면 호의 시작 점까지 선을 긋는다)
	// - 쓸어 가는 각: 시계 방향으로 끝 - 시작 >= 2π 이거나 반시계로 시작 - 끝 >= 2π 이면 한 바퀴, 아니면 그 방향으로 [0, 2π) 안의 각이다. (원본 규칙)
	//==============================================================================
	pub(crate) fn ellipse_with(&mut self, matrix: &CanvasMatrix, center_x: f32, center_y: f32, radius_x: f32, radius_y: f32, rotation: f32, start_angle: f32, end_angle: f32, is_counterclockwise: bool) -> bool {
		if radius_x < 0.0 || radius_y < 0.0 {
			return false;
		}
		let value_list = [center_x, center_y, radius_x, radius_y, rotation, start_angle, end_angle];
		if value_list.iter().any(|value| !value.is_finite()) {
			return true;
		}
		let full_turn = 2.0 * PI;
		let sweep_angle = if !is_counterclockwise && end_angle - start_angle >= full_turn {
			full_turn
		}
		else if is_counterclockwise && start_angle - end_angle >= full_turn {
			-full_turn
		}
		else if !is_counterclockwise {
			(end_angle - start_angle).rem_euclid(full_turn)
		}
		else {
			-((start_angle - end_angle).rem_euclid(full_turn))
		};
		// 타원 좌표 → 담는 좌표. (가운데로 옮기고 rotation 만큼 돌린 뒤 matrix)
		let ellipse_matrix = matrix.multiply(&CanvasMatrix::create(rotation.cos(), rotation.sin(), -rotation.sin(), rotation.cos(), center_x, center_y));
		let start_x = radius_x * start_angle.cos();
		let start_y = radius_y * start_angle.sin();
		if self.current_point.is_some() {
			self.push_line(&ellipse_matrix, start_x, start_y);
		}
		else {
			self.move_to_with(&ellipse_matrix, start_x, start_y);
		}
		if sweep_angle == 0.0 {
			return true;
		}
		let segment_count = (sweep_angle.abs() / (PI * 0.5)).ceil().max(1.0) as usize;
		let segment_angle = sweep_angle / segment_count as f32;
		let handle_length = 4.0 / 3.0 * (segment_angle / 4.0).tan();
		let mut angle = start_angle;
		for _ in 0..segment_count {
			let next_angle = angle + segment_angle;
			let (cosine, sine) = (angle.cos(), angle.sin());
			let (next_cosine, next_sine) = (next_angle.cos(), next_angle.sin());
			let first_control_x = radius_x * (cosine - handle_length * sine);
			let first_control_y = radius_y * (sine + handle_length * cosine);
			let second_control_x = radius_x * (next_cosine + handle_length * next_sine);
			let second_control_y = radius_y * (next_sine - handle_length * next_cosine);
			self.push_cubic(&ellipse_matrix, first_control_x, first_control_y, second_control_x, second_control_y, radius_x * next_cosine, radius_y * next_sine);
			angle = next_angle;
		}
		true
	}

	//==============================================================================
	// arcTo. (옮겨 담는 행렬과 함께. 지금 점은 역행렬로 되돌려 셈한다)
	//==============================================================================
	pub(crate) fn arc_to_with(&mut self, matrix: &CanvasMatrix, first_x: f32, first_y: f32, second_x: f32, second_y: f32, radius: f32) -> bool {
		if radius < 0.0 {
			return false;
		}
		if ![first_x, first_y, second_x, second_y, radius].iter().all(|value| value.is_finite()) {
			return true;
		}
		self.ensure_subpath(matrix, first_x, first_y);
		let (current_x, current_y) = match (self.current_point, matrix.invert()) {
			(Some(current_point), Some(inverse_matrix)) => {
				inverse_matrix.transform_point(current_point.0, current_point.1)
			}
			_ => {
				return true;
			}
		};
		let first_vector = (current_x - first_x, current_y - first_y);
		let second_vector = (second_x - first_x, second_y - first_y);
		let cross = first_vector.0 * second_vector.1 - first_vector.1 * second_vector.0;
		let is_degenerate = (current_x == first_x && current_y == first_y) || (first_x == second_x && first_y == second_y) || radius == 0.0 || cross.abs() < 1e-6;
		if is_degenerate {
			self.push_line(matrix, first_x, first_y);
			return true;
		}
		let first_length = (first_vector.0 * first_vector.0 + first_vector.1 * first_vector.1).sqrt();
		let second_length = (second_vector.0 * second_vector.0 + second_vector.1 * second_vector.1).sqrt();
		let cosine = ((first_vector.0 * second_vector.0 + first_vector.1 * second_vector.1) / (first_length * second_length)).clamp(-1.0, 1.0);
		let half_angle = cosine.acos() * 0.5;
		let tangent_distance = radius / half_angle.tan();
		let first_tangent = (first_x + first_vector.0 / first_length * tangent_distance, first_y + first_vector.1 / first_length * tangent_distance);
		let second_tangent = (first_x + second_vector.0 / second_length * tangent_distance, first_y + second_vector.1 / second_length * tangent_distance);
		// 원의 가운데: 꼭짓점에서 두 방향의 이등분선 쪽으로 radius / sin(반각).
		let bisector = (first_vector.0 / first_length + second_vector.0 / second_length, first_vector.1 / first_length + second_vector.1 / second_length);
		let bisector_length = (bisector.0 * bisector.0 + bisector.1 * bisector.1).sqrt();
		let center_distance = radius / half_angle.sin();
		let center = (first_x + bisector.0 / bisector_length * center_distance, first_y + bisector.1 / bisector_length * center_distance);
		let start_angle = (first_tangent.1 - center.1).atan2(first_tangent.0 - center.0);
		let end_angle = (second_tangent.1 - center.1).atan2(second_tangent.0 - center.0);
		let is_counterclockwise = cross > 0.0;
		self.ellipse_with(matrix, center.0, center.1, radius, radius, 0.0, start_angle, end_angle, is_counterclockwise);
		true
	}

	//==============================================================================
	// roundRect. (옮겨 담는 행렬과 함께. 원본 규칙: 반지름 1 개는 모두, 2 개는 [왼위와 오른아래, 오른위와 왼아래],
	// 3 개는 [왼위, 오른위와 왼아래, 오른아래], 4 개는 [왼위, 오른위, 오른아래, 왼아래]. 음수 폭과 높이는 뒤집고, 넘치는 반지름은 같은 비로 줄인다)
	//==============================================================================
	pub(crate) fn round_rect_with(&mut self, matrix: &CanvasMatrix, x: f32, y: f32, width: f32, height: f32, radius_list: &[f32]) -> bool {
		if radius_list.is_empty() || radius_list.len() > 4 || radius_list.iter().any(|radius| *radius < 0.0) {
			return false;
		}
		if ![x, y, width, height].iter().all(|value| value.is_finite()) || radius_list.iter().any(|radius| !radius.is_finite()) {
			return true;
		}
		let (mut top_left, mut top_right, mut bottom_right, mut bottom_left) = match radius_list.len() {
			1 => {
				(radius_list[0], radius_list[0], radius_list[0], radius_list[0])
			}
			2 => {
				(radius_list[0], radius_list[1], radius_list[0], radius_list[1])
			}
			3 => {
				(radius_list[0], radius_list[1], radius_list[2], radius_list[1])
			}
			_ => {
				(radius_list[0], radius_list[1], radius_list[2], radius_list[3])
			}
		};
		if width < 0.0 {
			std::mem::swap(&mut top_left, &mut top_right);
			std::mem::swap(&mut bottom_left, &mut bottom_right);
		}
		if height < 0.0 {
			std::mem::swap(&mut top_left, &mut bottom_left);
			std::mem::swap(&mut top_right, &mut bottom_right);
		}
		let absolute_width = width.abs();
		let absolute_height = height.abs();
		let mut scale: f32 = 1.0;
		for (side_length, radius_sum) in [(absolute_width, top_left + top_right), (absolute_height, top_right + bottom_right), (absolute_width, bottom_right + bottom_left), (absolute_height, bottom_left + top_left)] {
			if radius_sum > 0.0 {
				scale = scale.min(side_length / radius_sum);
			}
		}
		top_left *= scale;
		top_right *= scale;
		bottom_right *= scale;
		bottom_left *= scale;
		let left = x.min(x + width);
		let top = y.min(y + height);
		let right = left + absolute_width;
		let bottom = top + absolute_height;
		self.move_to_with(matrix, left + top_left, top);
		self.push_line(matrix, right - top_right, top);
		self.push_corner(matrix, right - top_right, top + top_right, top_right, -PI * 0.5);
		self.push_line(matrix, right, bottom - bottom_right);
		self.push_corner(matrix, right - bottom_right, bottom - bottom_right, bottom_right, 0.0);
		self.push_line(matrix, left + bottom_left, bottom);
		self.push_corner(matrix, left + bottom_left, bottom - bottom_left, bottom_left, PI * 0.5);
		self.push_line(matrix, left, top + top_left);
		self.push_corner(matrix, left + top_left, top + top_left, top_left, PI);
		self.close_path();
		self.move_to_with(matrix, x, y);
		true
	}

	//==============================================================================
	// 둥근 모서리 하나. (가운데와 반지름, 시작 각에서 시계 방향 90 도)
	//==============================================================================
	fn push_corner(&mut self, matrix: &CanvasMatrix, center_x: f32, center_y: f32, radius: f32, start_angle: f32) {
		if radius <= 0.0 {
			return;
		}
		let handle_length = 4.0 / 3.0 * (PI / 8.0).tan();
		let end_angle = start_angle + PI * 0.5;
		let (cosine, sine) = (start_angle.cos(), start_angle.sin());
		let (end_cosine, end_sine) = (end_angle.cos(), end_angle.sin());
		self.push_cubic(matrix, center_x + radius * (cosine - handle_length * sine), center_y + radius * (sine + handle_length * cosine), center_x + radius * (end_cosine + handle_length * end_sine), center_y + radius * (end_sine - handle_length * end_cosine), center_x + radius * end_cosine, center_y + radius * end_sine);
	}
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
				if !image.has_pixels() {
					eprintln!("Canvas2D: 그림의 CPU 픽셀이 없어 그리지 못합니다. (GPU 에 올린 뒤 놓은 그림, Image::set_pixels_retained(true) 로 남긴다)");
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
