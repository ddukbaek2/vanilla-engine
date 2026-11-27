//==============================================================================
// 해석적 AA 경로 래스터라이저. (크롬 CPU 스키아가 글리프 경로를 A8 마스크로 칠하는 셈)
// - 스키아 SkScalerContext::generateImageFromPath 가 부르는 SkScan::AntiFillPath 는 해석적 AA(SkScan_AAAPath)다.
//   가장자리는 SkAnalyticEdge(16.16 고정 소수, y 는 1/4 픽셀로 맞춤, 곡선은 앞으로 차분), 모으기는 SkAnalyticEdgeBuilder(y 극값에서 자르기, 세로선 합치기),
//   칠하기는 aaa_walk_edges(줄마다 사다리꼴 넓이를 알파로, 0 이 아닌 감김)다.
// - 작은 마스크(너비 32 이하, 줄 4 바이트 맞춤 넓이 1024 이하)는 MaskAdditiveBlitter, 그 밖은 SafeRLEAdditiveBlitter 의 셈이다.
//   (RLE 는 줄을 내보낼 때 248 이상을 255, 8 미만을 0 으로 맞추고, 꽉 찬 줄 몫은 A8 판에 곧바로 src-over 로 얹는다)
// - 스키아(BSD-3) 원문을 읽고 같은 셈을 엔진 꼴로 짰다. 볼록 경로 갈래(aaa_walk_convex_edges)와 사각형 갈래(blitFatAntiRect)는 아직 없고 모두 일반 갈래로 칠한다.
//==============================================================================

// 지금은 리눅스 글리프 마스크만 쓴다. (가짜 굵게의 경로 칠하기가 세 운영체제에 붙으면 뺀다)
#![cfg_attr(not(all(unix, not(target_os = "macos"), not(target_os = "ios"), not(target_os = "android"))), allow(dead_code))]

use rustybuzz::ttf_parser::OutlineBuilder;

/// 16.16 고정 소수.
type Fixed = i32;
/// 26.6 고정 소수.
type FixedDot6 = i32;

/// 고정 소수 1.
const FIXED_ONE: Fixed = 1 << 16;
/// 고정 소수 1/2.
const FIXED_HALF: Fixed = 1 << 15;
/// y 를 맞추는 정밀도. (1/4 픽셀, SkAnalyticEdge::kDefaultAccuracy)
const DEFAULT_ACCURACY: i32 = 2;
/// 곡선을 나누는 횟수의 상한 지수. (MAX_COEFF_SHIFT)
const MAXIMUM_COEFFICIENT_SHIFT: i32 = 6;
/// 빠른 역수 표의 크기. (kInverseTableSize)
const INVERSE_TABLE_SIZE: i32 = 1024;
/// 마스크 판의 너비 상한. (MaskAdditiveBlitter::kMAX_WIDTH)
const MASK_MAXIMUM_WIDTH: i32 = 32;
/// 마스크 판의 넓이 상한. (MaskAdditiveBlitter::kMAX_STORAGE, 바이트)
const MASK_MAXIMUM_STORAGE: i64 = 1024;
/// 래스터 한 변의 상한. (픽셀)
const MAXIMUM_RASTER_SIZE: i32 = 4096;
/// 목록에서 없는 자리.
const NO_EDGE: usize = usize::MAX;
/// 머리 지킴이 가장자리.
const HEAD_EDGE: usize = 0;
/// 꼬리 지킴이 가장자리.
const TAIL_EDGE: usize = 1;

//==============================================================================
/// 기기 좌표 경로의 한 마디. (픽셀, y 는 아래가 양수)
//==============================================================================
#[derive(Clone, Copy)]
enum PathElement {
	/// 새 윤곽 시작.
	MoveTo([f32; 2]),
	/// 곧은 선.
	LineTo([f32; 2]),
	/// 2 차 곡선. (조절점, 끝점)
	QuadTo([f32; 2], [f32; 2]),
	/// 3 차 곡선. (조절점 둘, 끝점)
	CubicTo([f32; 2], [f32; 2], [f32; 2]),
	/// 윤곽 닫기.
	Close,
}

//==============================================================================
/// 기기 좌표 경로. (스키아 SkPath 처럼 점과 마디를 차례로, 점 수는 이동점과 조절점까지 센다)
//==============================================================================
#[derive(Clone, Default)]
pub(crate) struct DevicePath {
	element_list: Vec<PathElement>,
	point_count: usize,
}

impl DevicePath {
	//==============================================================================
	/// 빈 경로.
	//==============================================================================
	pub(crate) fn new() -> DevicePath {
		DevicePath::default()
	}

	//==============================================================================
	/// 새 윤곽 시작.
	//==============================================================================
	pub(crate) fn move_to(&mut self, point: [f32; 2]) {
		self.element_list.push(PathElement::MoveTo(point));
		self.point_count += 1;
	}

	//==============================================================================
	/// 곧은 선.
	//==============================================================================
	pub(crate) fn line_to(&mut self, point: [f32; 2]) {
		self.element_list.push(PathElement::LineTo(point));
		self.point_count += 1;
	}

	//==============================================================================
	/// 2 차 곡선.
	//==============================================================================
	pub(crate) fn quad_to(&mut self, control_point: [f32; 2], point: [f32; 2]) {
		self.element_list.push(PathElement::QuadTo(control_point, point));
		self.point_count += 2;
	}

	//==============================================================================
	/// 3 차 곡선.
	//==============================================================================
	pub(crate) fn cubic_to(&mut self, first_control_point: [f32; 2], second_control_point: [f32; 2], point: [f32; 2]) {
		self.element_list.push(PathElement::CubicTo(first_control_point, second_control_point, point));
		self.point_count += 3;
	}

	//==============================================================================
	/// 윤곽 닫기.
	//==============================================================================
	pub(crate) fn close(&mut self) {
		self.element_list.push(PathElement::Close);
	}

	//==============================================================================
	/// 조절점까지 넣은 경계. ([왼쪽, 위, 오른쪽, 아래], SkPath::getBounds 처럼 모든 점의 최소, 최대. 점이 없으면 None)
	//==============================================================================
	pub(crate) fn read_bounds(&self) -> Option<[f32; 4]> {
		let mut bounds: Option<[f32; 4]> = None;
		let mut add_point = |point: [f32; 2]| {
			bounds = Some(match bounds {
				Some(bounds) => {
					[bounds[0].min(point[0]), bounds[1].min(point[1]), bounds[2].max(point[0]), bounds[3].max(point[1])]
				}
				None => {
					[point[0], point[1], point[0], point[1]]
				}
			});
		};
		for element in &self.element_list {
			match *element {
				PathElement::MoveTo(point) | PathElement::LineTo(point) => {
					add_point(point);
				}
				PathElement::QuadTo(control_point, point) => {
					add_point(control_point);
					add_point(point);
				}
				PathElement::CubicTo(first_control_point, second_control_point, point) => {
					add_point(first_control_point);
					add_point(second_control_point);
					add_point(point);
				}
				PathElement::Close => {}
			}
		}
		bounds
	}

	//==============================================================================
	/// 가장자리 조각 돌기. (SkPathEdgeIter 처럼 닫히지 않은 윤곽은 시작점으로 닫는 선을 더한다)
	//==============================================================================
	fn visit_edges(&self, offset_x: f32, offset_y: f32, visitor: &mut dyn FnMut(&EdgePiece)) {
		let shift = |point: [f32; 2]| -> [f32; 2] { [point[0] + offset_x, point[1] + offset_y] };
		let mut contour_start: Option<[f32; 2]> = None;
		let mut current_point = [0.0f32; 2];
		let mut needs_close_line = false;
		for element in &self.element_list {
			match *element {
				PathElement::MoveTo(point) => {
					if needs_close_line {
						if let Some(start_point) = contour_start {
							visitor(&EdgePiece::Line([current_point, start_point]));
						}
					}
					let point = shift(point);
					contour_start = Some(point);
					current_point = point;
					needs_close_line = false;
				}
				PathElement::LineTo(point) => {
					let point = shift(point);
					visitor(&EdgePiece::Line([current_point, point]));
					current_point = point;
					needs_close_line = contour_start != Some(point);
				}
				PathElement::QuadTo(control_point, point) => {
					let (control_point, point) = (shift(control_point), shift(point));
					visitor(&EdgePiece::Quad([current_point, control_point, point]));
					current_point = point;
					needs_close_line = contour_start != Some(point);
				}
				PathElement::CubicTo(first_control_point, second_control_point, point) => {
					let (first_control_point, second_control_point, point) = (shift(first_control_point), shift(second_control_point), shift(point));
					visitor(&EdgePiece::Cubic([current_point, first_control_point, second_control_point, point]));
					current_point = point;
					needs_close_line = contour_start != Some(point);
				}
				PathElement::Close => {
					if needs_close_line {
						if let Some(start_point) = contour_start {
							visitor(&EdgePiece::Line([current_point, start_point]));
							current_point = start_point;
						}
					}
					needs_close_line = false;
				}
			}
		}
		if needs_close_line {
			if let Some(start_point) = contour_start {
				visitor(&EdgePiece::Line([current_point, start_point]));
			}
		}
	}
}

//==============================================================================
/// 가장자리 조각 하나. (선, 2 차, 3 차 곡선의 점들)
//==============================================================================
enum EdgePiece {
	/// 선.
	Line([[f32; 2]; 2]),
	/// 2 차 곡선.
	Quad([[f32; 2]; 3]),
	/// 3 차 곡선.
	Cubic([[f32; 2]; 4]),
}

//==============================================================================
/// 글꼴 윤곽을 기기 경로로 옮기는 받개. (x' = x * 배율 + 옮김 x, y' = -y * 배율 + 옮김 y)
//==============================================================================
pub(crate) struct DevicePathCollector<'path> {
	/// 받을 경로.
	pub path: &'path mut DevicePath,
	/// 글꼴 단위에서 픽셀로의 배율.
	pub scale: f32,
}

impl DevicePathCollector<'_> {
	//==============================================================================
	/// 점 옮기기. (위가 양수인 글꼴 좌표 → 아래가 양수인 기기 좌표)
	//==============================================================================
	fn transform(&self, position_x: f32, position_y: f32) -> [f32; 2] {
		[position_x * self.scale, -(position_y * self.scale)]
	}
}

impl OutlineBuilder for DevicePathCollector<'_> {
	//==============================================================================
	/// 새 윤곽 시작.
	//==============================================================================
	fn move_to(&mut self, position_x: f32, position_y: f32) {
		let point = self.transform(position_x, position_y);
		self.path.move_to(point);
	}

	//==============================================================================
	/// 곧은 선.
	//==============================================================================
	fn line_to(&mut self, position_x: f32, position_y: f32) {
		let point = self.transform(position_x, position_y);
		self.path.line_to(point);
	}

	//==============================================================================
	/// 2 차 곡선.
	//==============================================================================
	fn quad_to(&mut self, control_x: f32, control_y: f32, position_x: f32, position_y: f32) {
		let control_point = self.transform(control_x, control_y);
		let point = self.transform(position_x, position_y);
		self.path.quad_to(control_point, point);
	}

	//==============================================================================
	/// 3 차 곡선.
	//==============================================================================
	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, position_x: f32, position_y: f32) {
		let first_control_point = self.transform(first_control_x, first_control_y);
		let second_control_point = self.transform(second_control_x, second_control_y);
		let point = self.transform(position_x, position_y);
		self.path.cubic_to(first_control_point, second_control_point, point);
	}

	//==============================================================================
	/// 윤곽 닫기.
	//==============================================================================
	fn close(&mut self) {
		self.path.close();
	}
}

//==============================================================================
/// 윤곽을 해석적 AA 로 칠한 덮인 정도. (colorglyph::rasterize_outline 과 같은 꼴, 크롬 스키아의 글리프 경로 마스크)
/// - 상자는 스키아 GenerateMetricsFromPath 처럼 조절점까지 넣은 경계를 내림, 올림한 것이다.
///
/// # 인자
/// - `draw_outline`: 받은 OutlineBuilder 에 윤곽을 그리는 함수. (글꼴 단위, 위가 +, 그렸으면 true)
/// - `scale`: 글꼴 단위에서 픽셀로의 배율.
///
/// # 돌려줌
/// (덮인 정도, 너비, 높이, 펜 원점에서 왼쪽, 기준선에서 위). 그린 것이 없거나 상자가 비었거나 4096 을 넘으면 None.
//==============================================================================
pub(crate) fn rasterize_outline(draw_outline: &dyn Fn(&mut dyn OutlineBuilder) -> bool, scale: f32) -> Option<(Vec<u8>, i32, i32, i32, i32)> {
	let mut path = DevicePath::new();
	{
		let mut collector = DevicePathCollector {
			path: &mut path,
			scale,
		};
		if !draw_outline(&mut collector) {
			return None;
		}
	}
	let (coverage, width, height, left_x, top_y) = rasterize_path(&path)?;
	Some((coverage, width, height, left_x, -top_y))
}

//==============================================================================
/// 기기 경로를 해석적 AA 로 칠한 덮인 정도. (0 이 아닌 감김)
///
/// # 돌려줌
/// (덮인 정도, 너비, 높이, 왼쪽 x, 위 y). 기기 좌표의 정수 상자이고, 덮인 정도는 위 줄부터 픽셀마다 한 바이트다. 비었으면 None.
//==============================================================================
pub(crate) fn rasterize_path(path: &DevicePath) -> Option<(Vec<u8>, i32, i32, i32, i32)> {
	let (left_x, top_y, width, height) = read_device_box(path)?;
	let coverage = fill_path(path, [left_x, top_y, width, height]);
	Some((coverage, width, height, left_x, top_y))
}

//==============================================================================
/// 경로의 정수 상자. ((왼쪽, 위, 너비, 높이), 조절점까지 넣은 경계를 내림, 올림, 비었거나 4096 을 넘으면 None)
//==============================================================================
fn read_device_box(path: &DevicePath) -> Option<(i32, i32, i32, i32)> {
	let bounds = path.read_bounds()?;
	if !bounds.iter().all(|value| value.is_finite()) {
		return None;
	}
	let left_x = bounds[0].floor() as i32;
	let top_y = bounds[1].floor() as i32;
	let width = bounds[2].ceil() as i32 - left_x;
	let height = bounds[3].ceil() as i32 - top_y;
	if width <= 0 || height <= 0 || width > MAXIMUM_RASTER_SIZE || height > MAXIMUM_RASTER_SIZE {
		return None;
	}
	Some((left_x, top_y, width, height))
}

//==============================================================================
/// 잘라 내는 판 안에 경로 칠하기. (SkScan::AAAFillPath, 판은 [왼쪽, 위, 너비, 높이] 이고 경로 상자를 품어야 한다)
/// - 받개는 경로 상자로 고르고, 마스크 갈래는 칠하는 가로 범위를 경로 상자로, RLE 갈래는 판으로 둔다. (aaa_fill_path 의 leftBound, rightBound)
///
/// # 돌려줌
/// 판 크기의 덮인 정도. (위 줄부터)
//==============================================================================
fn fill_path(path: &DevicePath, clip: [i32; 4]) -> Vec<u8> {
	let [clip_left, clip_top, clip_width, clip_height] = clip;
	let mut blitter = CoverageBlitter::new(clip_width, clip_height, [0, 0, 0, 0], false);
	let Some((left_x, top_y, width, height)) = read_device_box(path)
	else {
		return blitter.finish();
	};
	let box_rect = [left_x - clip_left, top_y - clip_top, width, height];
	if box_rect[0] < 0 || box_rect[1] < 0 || box_rect[0] + width > clip_width || box_rect[1] + height > clip_height {
		return blitter.finish();
	}
	// 판 좌표로 옮긴다. (generateImageFromPath 의 setTranslate(-left, -top))
	let mut edge_list = build_edges(path, -(clip_left as f32), -(clip_top as f32));
	let is_using_mask = width <= MASK_MAXIMUM_WIDTH && ((width as i64 + 3) & !3) * height as i64 <= MASK_MAXIMUM_STORAGE;
	blitter = CoverageBlitter::new(clip_width, clip_height, box_rect, is_using_mask);
	if edge_list.len() > 2 {
		// 점이 줄 수의 두 배보다 많으면 교차를 따로 셈하지 않는다. (이미 잘게 나뉘었다고 본다)
		let is_intersect_skipped = path.point_count > (height as usize) * 2;
		let (left_clip, right_clip) = if is_using_mask {
			(box_rect[0] << 16, (box_rect[0] + width) << 16)
		}
		else {
			(0, clip_width << 16)
		};
		walk_edges(&mut edge_list, &mut blitter, box_rect[1], box_rect[1] + height, left_clip, right_clip, is_intersect_skipped);
	}
	blitter.finish()
}

//==============================================================================
// 고정 소수 셈. (스키아 SkFixed.h, SkFDot6.h 와 같은 정수 셈)
//==============================================================================

//==============================================================================
/// 26.6 을 16.16 으로. (SkFDot6ToFixed, 왼쪽으로 10)
//==============================================================================
fn fixed_dot6_to_fixed(value: FixedDot6) -> Fixed {
	(value as u32).wrapping_shl(10) as i32
}

//==============================================================================
/// 16.16 을 26.6 으로. (SkFixedToFDot6, 오른쪽으로 10)
//==============================================================================
fn fixed_to_fixed_dot6(value: Fixed) -> FixedDot6 {
	value >> 10
}

//==============================================================================
/// 16.16 곱. (SkFixedMul)
//==============================================================================
fn multiply_fixed(first: Fixed, second: Fixed) -> Fixed {
	((first as i64 * second as i64) >> 16) as i32
}

//==============================================================================
/// 16.16 나눗셈. (SkFixedDiv, 32 비트로 죔)
//==============================================================================
fn divide_fixed(numerator: i32, denominator: i32) -> Fixed {
	if denominator == 0 {
		return if numerator >= 0 {
			i32::MAX
		}
		else {
			i32::MIN
		};
	}
	(((numerator as i64) << 16) / denominator as i64).clamp(i32::MIN as i64, i32::MAX as i64) as i32
}

//==============================================================================
/// 26.6 나눗셈의 16.16 몫. (SkFDot6Div)
//==============================================================================
fn divide_fixed_dot6(numerator: FixedDot6, denominator: FixedDot6) -> Fixed {
	if denominator == 0 {
		return divide_fixed(numerator, denominator);
	}
	if numerator >= i16::MIN as i32 && numerator <= i16::MAX as i32 {
		((numerator as u32).wrapping_shl(16) as i32).wrapping_div(denominator)
	}
	else {
		divide_fixed(numerator, denominator)
	}
}

//==============================================================================
/// 26.6 의 빠른 역수. (quick_inverse 의 표는 2^22 / x 를 0 쪽으로 자른 값이다, |x| <= 1024)
//==============================================================================
fn quick_inverse(value: FixedDot6) -> Fixed {
	if value == 0 {
		0
	}
	else if value > 0 {
		(1 << 22) / value
	}
	else {
		-((1 << 22) / -value)
	}
}

//==============================================================================
/// 26.6 의 빠른 나눗셈. (quick_div, 나누는 수가 8 이상 1024 미만이고 나뉘는 수가 4096 미만이면 표로)
//==============================================================================
fn quick_divide(numerator: FixedDot6, denominator: FixedDot6) -> Fixed {
	const MINIMUM_BITS: i32 = 3;
	const MAXIMUM_BITS: i32 = 31;
	const MAXIMUM_ABSOLUTE_NUMERATOR: i32 = 1 << (MAXIMUM_BITS - (22 - MINIMUM_BITS));
	let absolute_numerator = numerator.wrapping_abs();
	let absolute_denominator = denominator.wrapping_abs();
	if absolute_denominator >= (1 << MINIMUM_BITS) && absolute_denominator < INVERSE_TABLE_SIZE && absolute_numerator < MAXIMUM_ABSOLUTE_NUMERATOR {
		return numerator.wrapping_mul(quick_inverse(denominator)) >> 6;
	}
	divide_fixed_dot6(numerator, denominator)
}

//==============================================================================
/// y 를 1/4 픽셀로 맞추기. (SkAnalyticEdge::SnapY, 부호 없는 셈)
//==============================================================================
fn snap_y(value: Fixed) -> Fixed {
	((value as u32).wrapping_add((FIXED_ONE >> (DEFAULT_ACCURACY + 1)) as u32) >> (16 - DEFAULT_ACCURACY) << (16 - DEFAULT_ACCURACY)) as i32
}

//==============================================================================
/// 16.16 내림, 올림, 반올림. (정수 또는 고정 소수로)
//==============================================================================
fn floor_fixed_to_int(value: Fixed) -> i32 {
	value >> 16
}

fn ceil_fixed_to_int(value: Fixed) -> i32 {
	value.wrapping_add(FIXED_ONE - 1) >> 16
}

fn round_fixed_to_int(value: Fixed) -> i32 {
	value.wrapping_add(FIXED_HALF) >> 16
}

fn floor_fixed_to_fixed(value: Fixed) -> Fixed {
	((value as u32) & 0xFFFF0000) as i32
}

fn ceil_fixed_to_fixed(value: Fixed) -> Fixed {
	((value.wrapping_add(FIXED_ONE - 1) as u32) & 0xFFFF0000) as i32
}

fn round_fixed_to_fixed(value: Fixed) -> Fixed {
	((value.wrapping_add(FIXED_HALF) as u32) & 0xFFFF0000) as i32
}

//==============================================================================
/// 실수 점을 26.6 으로. (int(x * 2^(6 + 정밀도)), 0 쪽으로 자름)
//==============================================================================
fn scalar_to_fixed_dot6(value: f32, shift: i32) -> FixedDot6 {
	(value * (1 << (shift + 6)) as f32) as i32
}

//==============================================================================
/// 가장자리 하나. (SkAnalyticEdge, 곡선이면 앞으로 차분할 상태를 함께)
//==============================================================================
#[derive(Clone)]
struct AnalyticEdge {
	next: usize,
	previous: usize,
	x: Fixed,
	dx: Fixed,
	upper_x: Fixed,
	y: Fixed,
	upper_y: Fixed,
	lower_y: Fixed,
	dy: Fixed,
	is_line_type: bool,
	curve_count: i32,
	curve_shift: i32,
	winding: i32,
	curve: CurveState,
}

//==============================================================================
/// 곡선 가장자리의 앞으로 차분 상태. (SkAnalyticQuadraticEdge, SkAnalyticCubicEdge)
//==============================================================================
#[derive(Clone, Copy)]
enum CurveState {
	/// 선.
	None,
	/// 2 차 곡선. (x, y, dx, dy, ddx, ddy, 끝 x, 끝 y, 맞춘 x, 맞춘 y)
	Quad {
		quad_x: Fixed,
		quad_y: Fixed,
		quad_dx: Fixed,
		quad_dy: Fixed,
		quad_ddx: Fixed,
		quad_ddy: Fixed,
		last_x: Fixed,
		last_y: Fixed,
		snapped_x: Fixed,
		snapped_y: Fixed,
	},
	/// 3 차 곡선. (x, y, dx, dy, ddx, ddy, dddx, dddy, 끝 x, 끝 y, 맞춘 y, 고정 소수로 내리는 몫)
	Cubic {
		cubic_x: Fixed,
		cubic_y: Fixed,
		cubic_dx: Fixed,
		cubic_dy: Fixed,
		cubic_ddx: Fixed,
		cubic_ddy: Fixed,
		cubic_dddx: Fixed,
		cubic_dddy: Fixed,
		last_x: Fixed,
		last_y: Fixed,
		snapped_y: Fixed,
		to_fixed_shift: i32,
	},
}

impl AnalyticEdge {
	//==============================================================================
	/// 빈 가장자리. (지킴이와 채우기 전)
	//==============================================================================
	fn empty() -> AnalyticEdge {
		AnalyticEdge {
			next: NO_EDGE,
			previous: NO_EDGE,
			x: 0,
			dx: 0,
			upper_x: 0,
			y: 0,
			upper_y: 0,
			lower_y: 0,
			dy: 0,
			is_line_type: true,
			curve_count: 0,
			curve_shift: 0,
			winding: 1,
			curve: CurveState::None,
		}
	}

	//==============================================================================
	/// y 로 가기. (한 픽셀 아래면 dx 를 더하고, 아니면 윗점에서 다시 셈한다)
	//==============================================================================
	fn go_y(&mut self, target_y: Fixed) {
		if target_y == self.y.wrapping_add(FIXED_ONE) {
			self.x = self.x.saturating_add(self.dx);
			self.y = target_y;
		}
		else if target_y != self.y {
			let value = self.upper_x as i64 + ((self.dx as i64 * target_y.wrapping_sub(self.upper_y) as i64) >> 16);
			self.x = value.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
			self.y = target_y;
		}
	}

	//==============================================================================
	/// y 로 가기. (1/2^y_shift 픽셀 아래)
	//==============================================================================
	fn go_y_shifted(&mut self, target_y: Fixed, y_shift: i32) {
		self.y = target_y;
		self.x = self.x.saturating_add(self.dx >> y_shift);
	}

	//==============================================================================
	/// 기울기와 dy 채우기. (setLine, updateLine 의 공통 끝)
	//==============================================================================
	fn fill_line(&mut self, x0: Fixed, y0: Fixed, y1: Fixed, slope: Fixed, delta_x: FixedDot6, delta_y: FixedDot6, absolute_slope: i32) {
		self.x = x0;
		self.dx = slope;
		self.upper_x = x0;
		self.y = y0;
		self.upper_y = y0;
		self.lower_y = y1;
		self.dy = if delta_x == 0 || slope == 0 {
			i32::MAX
		}
		else if absolute_slope < INVERSE_TABLE_SIZE {
			quick_inverse(absolute_slope)
		}
		else {
			quick_divide(delta_y, delta_x).wrapping_abs()
		};
	}

	//==============================================================================
	/// 선 가장자리로. (SkAnalyticEdge::setLine, 높이가 0 이면 false)
	//==============================================================================
	fn set_line(&mut self, first_point: [f32; 2], second_point: [f32; 2]) -> bool {
		let mut x0 = fixed_dot6_to_fixed(scalar_to_fixed_dot6(first_point[0], DEFAULT_ACCURACY)) >> DEFAULT_ACCURACY;
		let mut y0 = snap_y(fixed_dot6_to_fixed(scalar_to_fixed_dot6(first_point[1], DEFAULT_ACCURACY)) >> DEFAULT_ACCURACY);
		let mut x1 = fixed_dot6_to_fixed(scalar_to_fixed_dot6(second_point[0], DEFAULT_ACCURACY)) >> DEFAULT_ACCURACY;
		let mut y1 = snap_y(fixed_dot6_to_fixed(scalar_to_fixed_dot6(second_point[1], DEFAULT_ACCURACY)) >> DEFAULT_ACCURACY);
		let mut winding = 1;
		if y0 > y1 {
			std::mem::swap(&mut x0, &mut x1);
			std::mem::swap(&mut y0, &mut y1);
			winding = -1;
		}
		let delta_y = fixed_to_fixed_dot6(y1.wrapping_sub(y0));
		if delta_y == 0 {
			return false;
		}
		let delta_x = fixed_to_fixed_dot6(x1.wrapping_sub(x0));
		let slope = quick_divide(delta_x, delta_y);
		let absolute_slope = slope.wrapping_abs();
		self.fill_line(x0, y0, y1, slope, delta_x, delta_y, absolute_slope);
		self.is_line_type = true;
		self.curve_count = 0;
		self.winding = winding;
		self.curve_shift = 0;
		self.curve = CurveState::None;
		true
	}

	//==============================================================================
	/// 곡선의 다음 선 조각으로. (SkAnalyticEdge::updateLine, 기울기는 부르는 쪽이 셈한다)
	//==============================================================================
	fn update_line(&mut self, mut x0: Fixed, mut y0: Fixed, mut x1: Fixed, mut y1: Fixed, slope: Fixed) -> bool {
		if y0 > y1 {
			std::mem::swap(&mut x0, &mut x1);
			std::mem::swap(&mut y0, &mut y1);
			self.winding = -self.winding;
		}
		let delta_x = fixed_to_fixed_dot6(x1.wrapping_sub(x0));
		let delta_y = fixed_to_fixed_dot6(y1.wrapping_sub(y0));
		if delta_y == 0 {
			return false;
		}
		let absolute_slope = fixed_to_fixed_dot6(slope).wrapping_abs();
		self.fill_line(x0, y0, y1, slope, delta_x, delta_y, absolute_slope);
		true
	}

	//==============================================================================
	/// 2 차 곡선 가장자리로. (setQuadraticWithoutUpdate 와 setQuadratic, 높이가 0 이면 false)
	//==============================================================================
	fn set_quadratic(&mut self, point_list: &[[f32; 2]; 3]) -> bool {
		let shift_up = DEFAULT_ACCURACY;
		let mut x0 = scalar_to_fixed_dot6(point_list[0][0], shift_up);
		let mut y0 = scalar_to_fixed_dot6(point_list[0][1], shift_up);
		let x1 = scalar_to_fixed_dot6(point_list[1][0], shift_up);
		let y1 = scalar_to_fixed_dot6(point_list[1][1], shift_up);
		let mut x2 = scalar_to_fixed_dot6(point_list[2][0], shift_up);
		let mut y2 = scalar_to_fixed_dot6(point_list[2][1], shift_up);
		let mut winding = 1;
		if y0 > y2 {
			std::mem::swap(&mut x0, &mut x2);
			std::mem::swap(&mut y0, &mut y2);
			winding = -1;
		}
		let top = (y0 + 32) >> 6;
		let bottom = (y2 + 32) >> 6;
		if top == bottom {
			return false;
		}
		let mut shift = {
			let delta_x = ((x1 as u32).wrapping_shl(1) as i32).wrapping_sub(x0).wrapping_sub(x2) >> 2;
			let delta_y = ((y1 as u32).wrapping_shl(1) as i32).wrapping_sub(y0).wrapping_sub(y2) >> 2;
			calculate_difference_shift(delta_x, delta_y, shift_up)
		};
		if shift == 0 {
			shift = 1;
		}
		else if shift > MAXIMUM_COEFFICIENT_SHIFT {
			shift = MAXIMUM_COEFFICIENT_SHIFT;
		}
		self.winding = winding;
		self.is_line_type = false;
		self.curve_count = 1 << shift;
		self.curve_shift = shift - 1;
		let half_a_x = (x0.wrapping_sub(x1).wrapping_sub(x1).wrapping_add(x2) as u32).wrapping_shl(16 - 6 - 1) as i32;
		let half_b_x = fixed_dot6_to_fixed(x1.wrapping_sub(x0));
		let half_a_y = (y0.wrapping_sub(y1).wrapping_sub(y1).wrapping_add(y2) as u32).wrapping_shl(16 - 6 - 1) as i32;
		let half_b_y = fixed_dot6_to_fixed(y1.wrapping_sub(y0));
		let quad_x = fixed_dot6_to_fixed(x0) >> DEFAULT_ACCURACY;
		let quad_y = snap_y(fixed_dot6_to_fixed(y0) >> DEFAULT_ACCURACY);
		self.curve = CurveState::Quad {
			quad_x,
			quad_y,
			quad_dx: half_b_x.wrapping_add(half_a_x >> shift) >> DEFAULT_ACCURACY,
			quad_dy: half_b_y.wrapping_add(half_a_y >> shift) >> DEFAULT_ACCURACY,
			quad_ddx: (half_a_x >> (shift - 1)) >> DEFAULT_ACCURACY,
			quad_ddy: (half_a_y >> (shift - 1)) >> DEFAULT_ACCURACY,
			last_x: fixed_dot6_to_fixed(x2) >> DEFAULT_ACCURACY,
			last_y: snap_y(fixed_dot6_to_fixed(y2) >> DEFAULT_ACCURACY),
			snapped_x: quad_x,
			snapped_y: quad_y,
		};
		self.update_quadratic()
	}

	//==============================================================================
	/// 2 차 곡선의 다음 선 조각. (SkAnalyticQuadraticEdge::updateQuadratic, 높이가 있는 조각을 찾으면 true)
	//==============================================================================
	fn update_quadratic(&mut self) -> bool {
		let CurveState::Quad {
			quad_x,
			quad_y,
			quad_dx,
			quad_dy,
			quad_ddx,
			quad_ddy,
			last_x,
			last_y,
			snapped_x,
			snapped_y,
		} = self.curve
		else {
			return false;
		};
		let mut is_success = false;
		let mut count = self.curve_count;
		let mut old_x = quad_x;
		let mut old_y = quad_y;
		let mut delta_x = quad_dx;
		let mut delta_y = quad_dy;
		let shift = self.curve_shift;
		let (mut new_x, mut new_y);
		// 맞춘 점은 고리가 끝난 뒤에만 바꾼다. (높이 없는 조각을 건너뛰면 다음 조각이 처음 맞춘 점에서 잇는다)
		let (mut new_snapped_x, mut new_snapped_y);
		loop {
			let slope;
			count -= 1;
			if count > 0 {
				new_x = old_x.wrapping_add(delta_x >> shift);
				new_y = old_y.wrapping_add(delta_y >> shift);
				if (delta_y >> shift).wrapping_abs() >= FIXED_ONE * 2 && ((delta_y.wrapping_abs() as i64) << 6) > delta_x.wrapping_abs() as i64 {
					let difference_y = fixed_to_fixed_dot6(new_y.wrapping_sub(snapped_y));
					slope = if difference_y != 0 {
						quick_divide(fixed_to_fixed_dot6(new_x.wrapping_sub(snapped_x)), difference_y)
					}
					else {
						i32::MAX
					};
					new_snapped_y = last_y.min(round_fixed_to_fixed(new_y));
					new_snapped_x = new_x.wrapping_sub(multiply_fixed(slope, new_y.wrapping_sub(new_snapped_y)));
				}
				else {
					new_snapped_y = last_y.min(snap_y(new_y));
					new_snapped_x = new_x;
					let difference_y = fixed_to_fixed_dot6(new_snapped_y.wrapping_sub(snapped_y));
					slope = if difference_y != 0 {
						quick_divide(fixed_to_fixed_dot6(new_x.wrapping_sub(snapped_x)), difference_y)
					}
					else {
						i32::MAX
					};
				}
				delta_x = delta_x.wrapping_add(quad_ddx);
				delta_y = delta_y.wrapping_add(quad_ddy);
			}
			else {
				new_x = last_x;
				new_y = last_y;
				new_snapped_y = new_y;
				new_snapped_x = new_x;
				let difference_y = fixed_to_fixed_dot6(new_y.wrapping_sub(snapped_y));
				slope = if difference_y != 0 {
					quick_divide(fixed_to_fixed_dot6(new_x.wrapping_sub(snapped_x)), difference_y)
				}
				else {
					i32::MAX
				};
			}
			if slope < i32::MAX {
				is_success = self.update_line(snapped_x, snapped_y, new_snapped_x, new_snapped_y, slope);
			}
			old_x = new_x;
			old_y = new_y;
			if !(count > 0 && !is_success) {
				break;
			}
		}
		self.curve = CurveState::Quad {
			quad_x: new_x,
			quad_y: new_y,
			quad_dx: delta_x,
			quad_dy: delta_y,
			quad_ddx,
			quad_ddy,
			last_x,
			last_y,
			snapped_x: new_snapped_x,
			snapped_y: new_snapped_y,
		};
		self.curve_count = count;
		is_success
	}

	//==============================================================================
	/// 3 차 곡선 가장자리로. (setCubicWithoutUpdate 와 setCubic, 높이가 0 이면 false)
	//==============================================================================
	fn set_cubic(&mut self, point_list: &[[f32; 2]; 4]) -> bool {
		let shift_up = DEFAULT_ACCURACY;
		let mut x0 = scalar_to_fixed_dot6(point_list[0][0], shift_up);
		let mut y0 = scalar_to_fixed_dot6(point_list[0][1], shift_up);
		let mut x1 = scalar_to_fixed_dot6(point_list[1][0], shift_up);
		let mut y1 = scalar_to_fixed_dot6(point_list[1][1], shift_up);
		let mut x2 = scalar_to_fixed_dot6(point_list[2][0], shift_up);
		let mut y2 = scalar_to_fixed_dot6(point_list[2][1], shift_up);
		let mut x3 = scalar_to_fixed_dot6(point_list[3][0], shift_up);
		let mut y3 = scalar_to_fixed_dot6(point_list[3][1], shift_up);
		let mut winding = 1;
		if y0 > y3 {
			std::mem::swap(&mut x0, &mut x3);
			std::mem::swap(&mut x1, &mut x2);
			std::mem::swap(&mut y0, &mut y3);
			std::mem::swap(&mut y1, &mut y2);
			winding = -1;
		}
		let top = (y0 + 32) >> 6;
		let bottom = (y3 + 32) >> 6;
		if top == bottom {
			return false;
		}
		let mut shift = {
			let delta_x = calculate_cubic_delta_from_line(x0, x1, x2, x3);
			let delta_y = calculate_cubic_delta_from_line(y0, y1, y2, y3);
			calculate_difference_shift(delta_x, delta_y, 2) + 1
		};
		if shift > MAXIMUM_COEFFICIENT_SHIFT {
			shift = MAXIMUM_COEFFICIENT_SHIFT;
		}
		let mut up_shift = 6;
		let mut down_shift = shift + up_shift - 10;
		if down_shift < 0 {
			down_shift = 0;
			up_shift = 10 - shift;
		}
		self.winding = winding;
		self.is_line_type = false;
		self.curve_count = -(1 << shift);
		self.curve_shift = shift;
		let shift_left = |value: i32| -> i32 { (value as u32).wrapping_shl(up_shift as u32) as i32 };
		let coefficients = |p0: i32, p1: i32, p2: i32, p3: i32| -> (Fixed, Fixed, Fixed, Fixed) {
			let b = shift_left(3i32.wrapping_mul(p1.wrapping_sub(p0)));
			let c = shift_left(3i32.wrapping_mul(p0.wrapping_sub(p1).wrapping_sub(p1).wrapping_add(p2)));
			let d = shift_left(p3.wrapping_add(3i32.wrapping_mul(p1.wrapping_sub(p2))).wrapping_sub(p0));
			let first = b.wrapping_add(c >> shift).wrapping_add(d >> (2 * shift));
			let second = (2i64 * c as i64 + ((3i64 * d as i64) >> (shift - 1))) as i32;
			let third = ((3i64 * d as i64) >> (shift - 1)) as i32;
			(fixed_dot6_to_fixed(p0), first, second, third)
		};
		let (cubic_x, cubic_dx, cubic_ddx, cubic_dddx) = coefficients(x0, x1, x2, x3);
		let (cubic_y, cubic_dy, cubic_ddy, cubic_dddy) = coefficients(y0, y1, y2, y3);
		let cubic_y = snap_y(cubic_y >> DEFAULT_ACCURACY);
		self.curve = CurveState::Cubic {
			cubic_x: cubic_x >> DEFAULT_ACCURACY,
			cubic_y,
			cubic_dx: cubic_dx >> DEFAULT_ACCURACY,
			cubic_dy: cubic_dy >> DEFAULT_ACCURACY,
			cubic_ddx: cubic_ddx >> DEFAULT_ACCURACY,
			cubic_ddy: cubic_ddy >> DEFAULT_ACCURACY,
			cubic_dddx: cubic_dddx >> DEFAULT_ACCURACY,
			cubic_dddy: cubic_dddy >> DEFAULT_ACCURACY,
			last_x: fixed_dot6_to_fixed(x3) >> DEFAULT_ACCURACY,
			last_y: snap_y(fixed_dot6_to_fixed(y3) >> DEFAULT_ACCURACY),
			snapped_y: cubic_y,
			to_fixed_shift: down_shift,
		};
		self.update_cubic()
	}

	//==============================================================================
	/// 3 차 곡선의 다음 선 조각. (SkAnalyticCubicEdge::updateCubic, 높이가 있는 조각을 찾으면 true)
	//==============================================================================
	fn update_cubic(&mut self) -> bool {
		let CurveState::Cubic {
			cubic_x,
			cubic_y,
			mut cubic_dx,
			mut cubic_dy,
			mut cubic_ddx,
			mut cubic_ddy,
			cubic_dddx,
			cubic_dddy,
			last_x,
			last_y,
			mut snapped_y,
			to_fixed_shift,
		} = self.curve
		else {
			return false;
		};
		let mut is_success;
		let mut count = self.curve_count;
		let mut old_x = cubic_x;
		let mut old_y = cubic_y;
		let (mut new_x, mut new_y);
		let second_shift = self.curve_shift;
		let first_shift = to_fixed_shift;
		loop {
			count += 1;
			if count < 0 {
				new_x = old_x.wrapping_add(cubic_dx >> first_shift);
				cubic_dx = cubic_dx.wrapping_add(cubic_ddx >> second_shift);
				cubic_ddx = cubic_ddx.wrapping_add(cubic_dddx);
				new_y = old_y.wrapping_add(cubic_dy >> first_shift);
				cubic_dy = cubic_dy.wrapping_add(cubic_ddy >> second_shift);
				cubic_ddy = cubic_ddy.wrapping_add(cubic_dddy);
			}
			else {
				new_x = last_x;
				new_y = last_y;
			}
			if new_y < old_y {
				new_y = old_y;
			}
			let mut new_snapped_y = snap_y(new_y);
			if last_y < new_snapped_y {
				new_snapped_y = last_y;
				count = 0;
			}
			let slope = if fixed_to_fixed_dot6(new_snapped_y.wrapping_sub(snapped_y)) == 0 {
				i32::MAX
			}
			else {
				divide_fixed_dot6(fixed_to_fixed_dot6(new_x.wrapping_sub(old_x)), fixed_to_fixed_dot6(new_snapped_y.wrapping_sub(snapped_y)))
			};
			is_success = self.update_line(old_x, snapped_y, new_x, new_snapped_y, slope);
			old_x = new_x;
			old_y = new_y;
			snapped_y = new_snapped_y;
			if !(count < 0 && !is_success) {
				break;
			}
		}
		self.curve = CurveState::Cubic {
			cubic_x: new_x,
			cubic_y: new_y,
			cubic_dx,
			cubic_dy,
			cubic_ddx,
			cubic_ddy,
			cubic_dddx,
			cubic_dddy,
			last_x,
			last_y,
			snapped_y,
			to_fixed_shift,
		};
		self.curve_count = count;
		is_success
	}

	//==============================================================================
	/// 곡선을 지금 자리에서 잇기. (keepContinuous: 2 차는 맞춘 x, y 를, 3 차는 x 와 맞춘 y 를 지금 자리로)
	//==============================================================================
	fn keep_continuous(&mut self) {
		let (current_x, current_y) = (self.x, self.y);
		match &mut self.curve {
			CurveState::Quad {
				snapped_x,
				snapped_y,
				..
			} => {
				*snapped_x = current_x;
				*snapped_y = current_y;
			}
			CurveState::Cubic {
				cubic_x,
				snapped_y,
				..
			} => {
				*cubic_x = current_x;
				*snapped_y = current_y;
			}
			CurveState::None => {}
		}
	}
}

//==============================================================================
/// 곡선을 몇 번 나눌지의 지수. (diff_to_shift, 가운데의 어긋남을 싸게 잰 거리로)
//==============================================================================
fn calculate_difference_shift(delta_x: FixedDot6, delta_y: FixedDot6, shift_aa: i32) -> i32 {
	let absolute_x = delta_x.wrapping_abs();
	let absolute_y = delta_y.wrapping_abs();
	let mut distance = if absolute_x > absolute_y {
		absolute_x + (absolute_y >> 1)
	}
	else {
		absolute_y + (absolute_x >> 1)
	};
	distance = (distance + (1 << (2 + shift_aa))) >> (3 + shift_aa);
	(32 - (distance as u32).leading_zeros() as i32) >> 1
}

//==============================================================================
/// 3 차 곡선이 선에서 벗어난 정도. (cubic_delta_from_line, 1/3 과 2/3 자리, 1/27 을 19/512 로)
//==============================================================================
fn calculate_cubic_delta_from_line(a: FixedDot6, b: FixedDot6, c: FixedDot6, d: FixedDot6) -> FixedDot6 {
	let one_third = (a.wrapping_mul(8).wrapping_sub(b.wrapping_mul(15)).wrapping_add(c.wrapping_mul(6)).wrapping_add(d)).wrapping_mul(19) >> 9;
	let two_third = (a.wrapping_add(b.wrapping_mul(6)).wrapping_sub(c.wrapping_mul(15)).wrapping_add(d.wrapping_mul(8))).wrapping_mul(19) >> 9;
	one_third.wrapping_abs().max(two_third.wrapping_abs())
}

//==============================================================================
// 곡선 자르기. (스키아 SkGeometry 의 y 극값 자르기)
//==============================================================================

//==============================================================================
/// 세 값이 단조가 아닌지. (is_not_monotonic)
//==============================================================================
fn is_not_monotonic(a: f32, b: f32, c: f32) -> bool {
	let ab = a - b;
	let mut bc = b - c;
	if ab < 0.0 {
		bc = -bc;
	}
	ab == 0.0 || bc < 0.0
}

//==============================================================================
/// 0 과 1 사이의 몫. (valid_unit_divide, 그 사이가 아니면 None)
//==============================================================================
fn divide_unit(mut numerator: f32, mut denominator: f32) -> Option<f32> {
	if numerator < 0.0 {
		numerator = -numerator;
		denominator = -denominator;
	}
	if denominator == 0.0 || numerator == 0.0 || numerator >= denominator {
		return None;
	}
	let ratio = numerator / denominator;
	if ratio.is_nan() || ratio == 0.0 {
		return None;
	}
	Some(ratio)
}

//==============================================================================
/// 0 과 1 사이의 2 차 방정식 근. (SkFindUnitQuadRoots, 작은 것부터)
//==============================================================================
fn find_unit_quad_roots(a: f32, b: f32, c: f32) -> Vec<f32> {
	if a == 0.0 {
		return divide_unit(-c, b).into_iter().collect();
	}
	let discriminant = b as f64 * b as f64 - 4.0 * a as f64 * c as f64;
	if discriminant < 0.0 {
		return Vec::new();
	}
	let root = discriminant.sqrt() as f32;
	if !root.is_finite() {
		return Vec::new();
	}
	let q = if b < 0.0 {
		-(b - root) / 2.0
	}
	else {
		-(b + root) / 2.0
	};
	let mut root_list: Vec<f32> = Vec::with_capacity(2);
	if let Some(value) = divide_unit(q, a) {
		root_list.push(value);
	}
	if let Some(value) = divide_unit(c, q) {
		root_list.push(value);
	}
	if root_list.len() == 2 {
		if root_list[0] > root_list[1] {
			root_list.swap(0, 1);
		}
		else if root_list[0] == root_list[1] {
			root_list.pop();
		}
	}
	root_list
}

//==============================================================================
/// 두 점 사이. (interp, v0 + (v1 - v0) * t)
//==============================================================================
fn interpolate(first: [f32; 2], second: [f32; 2], t: f32) -> [f32; 2] {
	[first[0] + (second[0] - first[0]) * t, first[1] + (second[1] - first[1]) * t]
}

//==============================================================================
/// 두 점 사이. (unchecked_mix, (b - a) * t + a)
//==============================================================================
fn mix_unchecked(first: [f32; 2], second: [f32; 2], t: f32) -> [f32; 2] {
	[(second[0] - first[0]) * t + first[0], (second[1] - first[1]) * t + first[1]]
}

//==============================================================================
/// 2 차 곡선을 y 단조인 조각으로. (SkChopQuadAtYExtrema)
//==============================================================================
fn chop_quad_at_y_extrema(point_list: &[[f32; 2]; 3]) -> Vec<[[f32; 2]; 3]> {
	let a = point_list[0][1];
	let mut b = point_list[1][1];
	let c = point_list[2][1];
	if is_not_monotonic(a, b, c) {
		if let Some(t) = divide_unit(a - b, a - b - b + c) {
			let p01 = interpolate(point_list[0], point_list[1], t);
			let p12 = interpolate(point_list[1], point_list[2], t);
			let middle = interpolate(p01, p12, t);
			// y 극값을 평평하게. (flatten_double_quad_extrema)
			let flat_y = middle[1];
			return vec![[point_list[0], [p01[0], flat_y], middle], [middle, [p12[0], flat_y], point_list[2]]];
		}
		b = if (a - b).abs() < (b - c).abs() {
			a
		}
		else {
			c
		};
	}
	vec![[point_list[0], [point_list[1][0], b], point_list[2]]]
}

//==============================================================================
/// 3 차 곡선을 t 하나에서 자르기. (SkChopCubicAt 의 한 자리, 일곱 점)
//==============================================================================
fn chop_cubic_at(point_list: &[[f32; 2]; 4], t: f32) -> [[f32; 2]; 7] {
	if t == 1.0 {
		return [point_list[0], point_list[1], point_list[2], point_list[3], point_list[3], point_list[3], point_list[3]];
	}
	let ab = mix_unchecked(point_list[0], point_list[1], t);
	let bc = mix_unchecked(point_list[1], point_list[2], t);
	let cd = mix_unchecked(point_list[2], point_list[3], t);
	let abc = mix_unchecked(ab, bc, t);
	let bcd = mix_unchecked(bc, cd, t);
	let abcd = mix_unchecked(abc, bcd, t);
	[point_list[0], ab, abc, abcd, bcd, cd, point_list[3]]
}

//==============================================================================
/// 3 차 곡선을 y 단조인 조각으로. (SkChopCubicAtYExtrema, 근이 둘이면 두 자리를 한 번에 자르는 SkChopCubicAt 의 셈)
//==============================================================================
fn chop_cubic_at_y_extrema(point_list: &[[f32; 2]; 4]) -> Vec<[[f32; 2]; 4]> {
	let a = point_list[0][1];
	let b = point_list[1][1];
	let c = point_list[2][1];
	let d = point_list[3][1];
	let root_list = find_unit_quad_roots(d - a + 3.0 * (b - c), 2.0 * (a - b - b + c), b - a);
	let mut destination: Vec<[f32; 2]> = Vec::with_capacity(10);
	match root_list.len() {
		0 => {
			return vec![*point_list];
		}
		1 => {
			destination.extend_from_slice(&chop_cubic_at(point_list, root_list[0]));
		}
		_ => {
			let (first_t, second_t) = (root_list[0], root_list[1]);
			if second_t == 1.0 {
				destination.extend_from_slice(&chop_cubic_at(point_list, first_t));
				destination.extend_from_slice(&[point_list[3], point_list[3], point_list[3]]);
			}
			else {
				let mix_pair = |first: [f32; 2], second: [f32; 2]| -> ([f32; 2], [f32; 2]) { (mix_unchecked(first, second, first_t), mix_unchecked(first, second, second_t)) };
				let (first_ab, second_ab) = mix_pair(point_list[0], point_list[1]);
				let (first_bc, second_bc) = mix_pair(point_list[1], point_list[2]);
				let (first_cd, second_cd) = mix_pair(point_list[2], point_list[3]);
				let first_abc = mix_unchecked(first_ab, first_bc, first_t);
				let second_abc = mix_unchecked(second_ab, second_bc, second_t);
				let first_bcd = mix_unchecked(first_bc, first_cd, first_t);
				let second_bcd = mix_unchecked(second_bc, second_cd, second_t);
				let first_abcd = mix_unchecked(first_abc, first_bcd, first_t);
				let second_abcd = mix_unchecked(second_abc, second_bcd, second_t);
				let first_middle = mix_unchecked(first_abc, first_bcd, second_t);
				let second_middle = mix_unchecked(second_abc, second_bcd, first_t);
				destination.extend_from_slice(&[point_list[0], first_ab, first_abc, first_abcd, first_middle, second_middle, second_abcd, second_bcd, second_cd, point_list[3]]);
			}
		}
	}
	// y 극값을 평평하게. (flatten_double_cubic_extrema)
	let flatten = |destination: &mut Vec<[f32; 2]>, start: usize| {
		let flat_y = destination[start + 3][1];
		destination[start + 2][1] = flat_y;
		destination[start + 4][1] = flat_y;
	};
	flatten(&mut destination, 0);
	if root_list.len() == 2 {
		flatten(&mut destination, 3);
	}
	let piece_count = root_list.len() + 1;
	(0..piece_count).map(|piece_index| [destination[piece_index * 3], destination[piece_index * 3 + 1], destination[piece_index * 3 + 2], destination[piece_index * 3 + 3]]).collect()
}

//==============================================================================
// 가장자리 모으기. (SkAnalyticEdgeBuilder::build, 잘라 낼 것이 없는 글리프 마스크)
//==============================================================================

//==============================================================================
/// 세로선 합치기 결과. (kNo_Combine, kPartial_Combine, kTotal_Combine)
//==============================================================================
#[derive(PartialEq)]
enum CombineResult {
	No,
	Partial,
	Total,
}

//==============================================================================
/// 새 세로선을 앞 세로선과 합치기. (SkAnalyticEdgeBuilder::combineVertical, 오른쪽으로 버릴 수 있는 경로)
//==============================================================================
fn combine_vertical(edge: &AnalyticEdge, last: &mut AnalyticEdge) -> CombineResult {
	let is_approximately_equal = |first: Fixed, second: Fixed| first.wrapping_sub(second).wrapping_abs() < 0x100;
	if !last.is_line_type || last.dx != 0 || edge.x != last.x {
		return CombineResult::No;
	}
	if edge.winding == last.winding {
		if edge.lower_y == last.upper_y {
			last.upper_y = edge.upper_y;
			last.y = last.upper_y;
			return CombineResult::Partial;
		}
		if is_approximately_equal(edge.upper_y, last.lower_y) {
			last.lower_y = edge.lower_y;
			return CombineResult::Partial;
		}
		return CombineResult::No;
	}
	if is_approximately_equal(edge.upper_y, last.upper_y) {
		if is_approximately_equal(edge.lower_y, last.lower_y) {
			return CombineResult::Total;
		}
		if edge.lower_y < last.lower_y {
			last.upper_y = edge.lower_y;
			last.y = last.upper_y;
			return CombineResult::Partial;
		}
		last.upper_y = last.lower_y;
		last.y = last.upper_y;
		last.lower_y = edge.lower_y;
		last.winding = edge.winding;
		return CombineResult::Partial;
	}
	if is_approximately_equal(edge.lower_y, last.lower_y) {
		if edge.upper_y > last.upper_y {
			last.lower_y = edge.upper_y;
			return CombineResult::Partial;
		}
		last.lower_y = last.upper_y;
		last.upper_y = edge.upper_y;
		last.y = last.upper_y;
		last.winding = edge.winding;
		return CombineResult::Partial;
	}
	CombineResult::No
}

//==============================================================================
/// 경로의 가장자리를 모아 차례로 잇기. (머리, 꼬리 지킴이가 0 과 1, 윗 y, x, dx 차례)
//==============================================================================
fn build_edges(path: &DevicePath, offset_x: f32, offset_y: f32) -> Vec<AnalyticEdge> {
	let mut edge_list: Vec<AnalyticEdge> = Vec::new();
	let add_line = |edge_list: &mut Vec<AnalyticEdge>, point_list: [[f32; 2]; 2]| {
		let mut edge = AnalyticEdge::empty();
		if !edge.set_line(point_list[0], point_list[1]) {
			return;
		}
		let combine_result = if edge.dx == 0 && edge.is_line_type && !edge_list.is_empty() {
			let last_index = edge_list.len() - 1;
			combine_vertical(&edge, &mut edge_list[last_index])
		}
		else {
			CombineResult::No
		};
		match combine_result {
			CombineResult::Total => {
				edge_list.pop();
			}
			CombineResult::Partial => {}
			CombineResult::No => {
				edge_list.push(edge);
			}
		}
	};
	path.visit_edges(offset_x, offset_y, &mut |piece| match piece {
		EdgePiece::Line(point_list) => {
			add_line(&mut edge_list, *point_list);
		}
		EdgePiece::Quad(point_list) => {
			for quad in chop_quad_at_y_extrema(point_list) {
				let mut edge = AnalyticEdge::empty();
				if edge.set_quadratic(&quad) {
					edge_list.push(edge);
				}
			}
		}
		EdgePiece::Cubic(point_list) => {
			for cubic in chop_cubic_at_y_extrema(point_list) {
				let mut edge = AnalyticEdge::empty();
				if edge.set_cubic(&cubic) {
					edge_list.push(edge);
				}
			}
		}
	});
	edge_list.sort_by(|first, second| first.upper_y.cmp(&second.upper_y).then(first.x.cmp(&second.x)).then(first.dx.cmp(&second.dx)));
	let mut head = AnalyticEdge::empty();
	head.upper_y = i32::MIN;
	head.lower_y = i32::MIN;
	head.x = i32::MIN;
	head.dy = i32::MAX;
	head.upper_x = i32::MIN;
	let mut tail = AnalyticEdge::empty();
	tail.upper_y = i32::MAX;
	tail.lower_y = i32::MAX;
	tail.x = i32::MAX;
	tail.dy = i32::MAX;
	tail.upper_x = i32::MAX;
	let mut linked_list = Vec::with_capacity(edge_list.len() + 2);
	linked_list.push(head);
	linked_list.push(tail);
	linked_list.extend(edge_list);
	let count = linked_list.len();
	if count == 2 {
		linked_list[HEAD_EDGE].next = TAIL_EDGE;
		linked_list[TAIL_EDGE].previous = HEAD_EDGE;
		return linked_list;
	}
	linked_list[HEAD_EDGE].next = 2;
	linked_list[2].previous = HEAD_EDGE;
	for index in 3..count {
		linked_list[index - 1].next = index;
		linked_list[index].previous = index - 1;
	}
	linked_list[count - 1].next = TAIL_EDGE;
	linked_list[TAIL_EDGE].previous = count - 1;
	linked_list
}

//==============================================================================
// 덮인 정도 받개. (MaskAdditiveBlitter 와 SafeRLEAdditiveBlitter, 그 뒤의 A8 판)
//==============================================================================

//==============================================================================
/// 0 ~ 255 를 넘지 않게 더하기. (safely_add_alpha)
//==============================================================================
fn add_alpha_safely(alpha: &mut u8, delta: u8) {
	*alpha = (*alpha as i32 + delta as i32).min(0xFF) as u8;
}

//==============================================================================
/// A8 판의 src-over. (SkA8_Blitter 의 srcover_p, 칠 색 알파 255 에 덮인 정도 aa)
//==============================================================================
fn blend_source_over(destination: &mut u8, coverage: u8) {
	if coverage == 0 {
		return;
	}
	let product = (255 - coverage as u32) * *destination as u32;
	*destination = (coverage as u32 + (((product + 128) * 257) >> 16)) as u8;
}

//==============================================================================
/// 덮인 정도 받개. (마스크 판에 곧바로 더하거나, 줄마다 더해 두었다 A8 판에 얹는다)
//==============================================================================
struct CoverageBlitter {
	/// A8 판 너비. (잘라 내는 판)
	width: i32,
	/// A8 판 높이.
	height: i32,
	/// 경로 상자. ([왼쪽, 위, 너비, 높이], 판 좌표)
	box_rect: [i32; 4],
	is_using_mask: bool,
	/// 마스크 판. (MaskAdditiveBlitter, 경로 상자 크기에 앞뒤 한 바이트씩 넘치는 자리를 둔다)
	mask_storage: Vec<u8>,
	/// A8 판.
	destination: Vec<u8>,
	/// 더해 둔 줄. (RLE 갈래, 경로 상자 너비)
	row_alpha: Vec<u8>,
	/// 더해 둔 줄의 y. (없으면 i32::MIN)
	row_y: i32,
}

impl CoverageBlitter {
	//==============================================================================
	/// 생성.
	//==============================================================================
	fn new(width: i32, height: i32, box_rect: [i32; 4], is_using_mask: bool) -> CoverageBlitter {
		CoverageBlitter {
			width,
			height,
			box_rect,
			is_using_mask,
			mask_storage: if is_using_mask {
				vec![0u8; (box_rect[2] * box_rect[3]) as usize + 2]
			}
			else {
				Vec::new()
			},
			destination: vec![0u8; (width * height) as usize],
			row_alpha: if is_using_mask {
				Vec::new()
			}
			else {
				vec![0u8; box_rect[2].max(0) as usize]
			},
			row_y: i32::MIN,
		}
	}

	//==============================================================================
	/// 마스크 판의 자리. (MaskAdditiveBlitter::getRow 의 줄 자리 + x, 판 밖은 None)
	//==============================================================================
	fn mask_index(&self, position_x: i32, row: i32) -> Option<usize> {
		let index = 1 + (row - self.box_rect[1]) as i64 * self.box_rect[2] as i64 + (position_x - self.box_rect[0]) as i64;
		if index < 0 || index >= self.mask_storage.len() as i64 {
			return None;
		}
		Some(index as usize)
	}

	//==============================================================================
	/// 마스크 판에 넣기. (is_set 이면 덮어쓰고 아니면 넘지 않게 더한다)
	//==============================================================================
	fn put_mask(&mut self, position_x: i32, row: i32, alpha: u8, is_set: bool) {
		if let Some(index) = self.mask_index(position_x, row) {
			if is_set {
				self.mask_storage[index] = alpha;
			}
			else {
				add_alpha_safely(&mut self.mask_storage[index], alpha);
			}
		}
	}

	//==============================================================================
	/// 더해 둔 줄 내보내기. (RunBasedAdditiveBlitter::flush, 248 이상은 255, 8 미만은 0 으로 맞춰 src-over)
	//==============================================================================
	fn flush(&mut self) {
		if self.row_y >= 0 && self.row_y < self.height {
			let row_start = (self.row_y * self.width) as usize;
			for column_index in 0..self.row_alpha.len() {
				let alpha = self.row_alpha[column_index];
				let snapped_alpha = if alpha > 247 {
					0xFF
				}
				else if alpha < 8 {
					0
				}
				else {
					alpha
				};
				let column = self.box_rect[0] + column_index as i32;
				if column >= 0 && column < self.width {
					blend_source_over(&mut self.destination[row_start + column as usize], snapped_alpha);
				}
				self.row_alpha[column_index] = 0;
			}
		}
		self.row_y = i32::MIN;
	}

	//==============================================================================
	/// 줄 바꾸기. (checkY)
	//==============================================================================
	fn check_y(&mut self, row: i32) {
		if row != self.row_y {
			self.flush();
			self.row_y = row;
		}
	}

	//==============================================================================
	/// 더해 둘 줄에 알파 여럿. (SafeRLEAdditiveBlitter::blitAntiH, 경로 상자 밖은 자른다)
	//==============================================================================
	fn add_row_alphas(&mut self, position_x: i32, row: i32, alpha_list: &[u8]) {
		self.check_y(row);
		let row_width = self.row_alpha.len() as i32;
		for (offset, alpha) in alpha_list.iter().enumerate() {
			let column = position_x - self.box_rect[0] + offset as i32;
			if column >= 0 && column < row_width {
				add_alpha_safely(&mut self.row_alpha[column as usize], *alpha);
			}
		}
	}

	//==============================================================================
	/// 더해 둘 줄에 같은 알파 여럿. (SafeRLEAdditiveBlitter::blitAntiH 의 너비 갈래)
	//==============================================================================
	fn add_row_run(&mut self, position_x: i32, row: i32, length: i32, alpha: u8) {
		self.check_y(row);
		let start = position_x - self.box_rect[0];
		for column in start.max(0)..(start + length).min(self.row_alpha.len() as i32) {
			add_alpha_safely(&mut self.row_alpha[column as usize], alpha);
		}
	}

	//==============================================================================
	/// A8 판에 곧바로 얹기. (진짜 받개의 blitV, blitAntiH, blitH)
	//==============================================================================
	fn blend_direct(&mut self, position_x: i32, row: i32, alpha: u8) {
		if position_x >= 0 && position_x < self.width && row >= 0 && row < self.height {
			blend_source_over(&mut self.destination[(row * self.width + position_x) as usize], alpha);
		}
	}

	//==============================================================================
	/// 마무리. (마스크 판은 0 인 판에 덮인 정도 그대로 놓고, RLE 는 남은 줄을 내보낸다)
	//==============================================================================
	fn finish(mut self) -> Vec<u8> {
		if self.is_using_mask {
			let [box_left, box_top, box_width, box_height] = self.box_rect;
			for row in 0..box_height {
				for column in 0..box_width {
					let value = self.mask_storage[(1 + row * box_width + column) as usize];
					self.destination[((box_top + row) * self.width + box_left + column) as usize] = value;
				}
			}
			return self.destination;
		}
		self.flush();
		self.destination
	}
}

//==============================================================================
// 사다리꼴 줄의 알파. (aaa 의 알파 셈, u8 은 스키아처럼 자른다)
//==============================================================================

//==============================================================================
/// 높이 1 인 사다리꼴의 알파. (trapezoid_to_alpha)
//==============================================================================
fn trapezoid_to_alpha(first_length: Fixed, second_length: Fixed) -> u8 {
	let area = first_length.wrapping_add(second_length) / 2;
	(area >> 8) as u8
}

//==============================================================================
/// 직각삼각형 (a, a * b) 의 알파. (partial_triangle_to_alpha 의 어림)
//==============================================================================
fn partial_triangle_to_alpha(a: Fixed, b: Fixed) -> u8 {
	let area = (a >> 11).wrapping_mul(a >> 11).wrapping_mul(b >> 11);
	((area >> 8) & 0xFF) as u8
}

//==============================================================================
/// 알파에 부분 높이 곱하기. (get_partial_alpha 의 고정 소수 갈래)
//==============================================================================
fn partial_alpha_by_height(alpha: u8, partial_height: Fixed) -> u8 {
	round_fixed_to_int((alpha as i32).wrapping_mul(partial_height)) as u8
}

//==============================================================================
/// 알파에 알파 곱하기. (get_partial_alpha 의 알파 갈래, >> 8)
//==============================================================================
fn partial_alpha_by_alpha(alpha: u8, full_alpha: u8) -> u8 {
	((alpha as i32 * full_alpha as i32) >> 8) as u8
}

//==============================================================================
/// 고정 소수 높이의 알파. (fixed_to_alpha)
//==============================================================================
fn fixed_to_alpha(value: Fixed) -> u8 {
	partial_alpha_by_height(0xFF, value)
}

//==============================================================================
/// 엇갈린 두 선의 교차 x 어림. (approximate_intersection)
//==============================================================================
fn approximate_intersection(mut l1: Fixed, mut r1: Fixed, mut l2: Fixed, mut r2: Fixed) -> Fixed {
	if l1 > r1 {
		std::mem::swap(&mut l1, &mut r1);
	}
	if l2 > r2 {
		std::mem::swap(&mut l2, &mut r2);
	}
	l1.max(l2).wrapping_add(r1.min(r2)) / 2
}

//==============================================================================
/// 선 위쪽 알파. (compute_alpha_above_line, l < 1)
//==============================================================================
fn compute_alpha_above_line(alpha_list: &mut [u8], l: Fixed, r: Fixed, delta_y: Fixed, full_alpha: u8) {
	let right_count = ceil_fixed_to_int(r);
	if right_count <= 0 {
		return;
	}
	if right_count == 1 {
		let alpha = ((right_count << 17).wrapping_sub(l).wrapping_sub(r) >> 9) as u8;
		alpha_list[0] = partial_alpha_by_alpha(alpha, full_alpha);
		return;
	}
	let first = FIXED_ONE - l;
	let last = r - ((right_count - 1) << 16);
	let first_height = multiply_fixed(first, delta_y);
	alpha_list[0] = (multiply_fixed(first, first_height) >> 9) as u8;
	let mut alpha16 = first_height.saturating_add(delta_y >> 1);
	for index in 1..(right_count - 1) as usize {
		alpha_list[index] = (alpha16 >> 8) as u8;
		alpha16 = alpha16.saturating_add(delta_y);
	}
	alpha_list[(right_count - 1) as usize] = (full_alpha as i32 - partial_triangle_to_alpha(last, delta_y) as i32) as u8;
}

//==============================================================================
/// 선 아래쪽 알파. (compute_alpha_below_line, l < 1)
//==============================================================================
fn compute_alpha_below_line(alpha_list: &mut [u8], l: Fixed, r: Fixed, delta_y: Fixed, full_alpha: u8) {
	let right_count = ceil_fixed_to_int(r);
	if right_count <= 0 {
		return;
	}
	if right_count == 1 {
		alpha_list[0] = partial_alpha_by_alpha(trapezoid_to_alpha(l, r), full_alpha);
		return;
	}
	let first = FIXED_ONE - l;
	let last = r - ((right_count - 1) << 16);
	let last_height = multiply_fixed(last, delta_y);
	alpha_list[(right_count - 1) as usize] = (multiply_fixed(last, last_height) >> 9) as u8;
	let mut alpha16 = last_height.saturating_add(delta_y >> 1);
	let mut index = right_count - 2;
	while index > 0 {
		alpha_list[index as usize] = ((alpha16 >> 8) & 0xFF) as u8;
		alpha16 = alpha16.saturating_add(delta_y);
		index -= 1;
	}
	alpha_list[0] = (full_alpha as i32 - partial_triangle_to_alpha(first, delta_y) as i32) as u8;
}

//==============================================================================
/// 알파 하나. (blit_single_alpha, 꽉 찬 줄이고 진짜 받개를 써도 되면 덮어쓴다)
//==============================================================================
fn blit_single_alpha(blitter: &mut CoverageBlitter, row: i32, position_x: i32, alpha: u8, full_alpha: u8, is_no_real_blitter: bool) {
	if blitter.is_using_mask {
		if full_alpha == 0xFF && !is_no_real_blitter {
			blitter.put_mask(position_x, row, alpha, true);
		}
		else {
			blitter.put_mask(position_x, row, partial_alpha_by_alpha(alpha, full_alpha), false);
		}
	}
	else if full_alpha == 0xFF && !is_no_real_blitter {
		blitter.blend_direct(position_x, row, alpha);
	}
	else {
		blitter.add_row_alphas(position_x, row, &[partial_alpha_by_alpha(alpha, full_alpha)]);
	}
}

//==============================================================================
/// 알파 둘. (blit_two_alphas)
//==============================================================================
fn blit_two_alphas(blitter: &mut CoverageBlitter, row: i32, position_x: i32, first_alpha: u8, second_alpha: u8, full_alpha: u8, is_no_real_blitter: bool) {
	if blitter.is_using_mask {
		blitter.put_mask(position_x, row, first_alpha, false);
		blitter.put_mask(position_x + 1, row, second_alpha, false);
	}
	else if full_alpha == 0xFF && !is_no_real_blitter {
		blitter.blend_direct(position_x, row, first_alpha);
		blitter.blend_direct(position_x + 1, row, second_alpha);
	}
	else {
		blitter.add_row_alphas(position_x, row, &[first_alpha]);
		blitter.add_row_alphas(position_x + 1, row, &[second_alpha]);
	}
}

//==============================================================================
/// 꽉 찬 알파 여럿. (blit_full_alpha)
//==============================================================================
fn blit_full_alpha(blitter: &mut CoverageBlitter, row: i32, position_x: i32, length: i32, full_alpha: u8, is_no_real_blitter: bool) {
	if blitter.is_using_mask {
		for offset in 0..length {
			blitter.put_mask(position_x + offset, row, full_alpha, false);
		}
	}
	else if full_alpha == 0xFF && !is_no_real_blitter {
		for offset in 0..length {
			blitter.blend_direct(position_x + offset, row, 0xFF);
		}
	}
	else {
		blitter.add_row_run(position_x, row, length, full_alpha);
	}
}

//==============================================================================
/// 사다리꼴 줄의 알파를 픽셀마다 셈해 얹기. (blit_aaa_trapezoid_row)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn blit_aaa_trapezoid_row(blitter: &mut CoverageBlitter, row: i32, ul: Fixed, ur: Fixed, ll: Fixed, lr: Fixed, left_delta_y: Fixed, right_delta_y: Fixed, full_alpha: u8, is_no_real_blitter: bool) {
	let left_index = floor_fixed_to_int(ul);
	let right_index = ceil_fixed_to_int(lr);
	let length = right_index - left_index;
	if length == 1 {
		let alpha = trapezoid_to_alpha(ur.wrapping_sub(ul), lr.wrapping_sub(ll));
		blit_single_alpha(blitter, row, left_index, alpha, full_alpha, is_no_real_blitter);
		return;
	}
	if length <= 0 || length > MAXIMUM_RASTER_SIZE + 2 {
		return;
	}
	let mut alpha_list = vec![full_alpha; length as usize];
	let mut temporary_list = vec![0u8; length as usize + 1];
	let upper_left = floor_fixed_to_int(ul);
	let lower_left = ceil_fixed_to_int(ll);
	if upper_left + 2 == lower_left {
		let first = (upper_left << 16) + FIXED_ONE - ul;
		let second = ll - ul - first;
		let first_alpha = (full_alpha as i32 - partial_triangle_to_alpha(first, left_delta_y) as i32) as u8;
		let second_alpha = partial_triangle_to_alpha(second, left_delta_y);
		alpha_list[0] = alpha_list[0].saturating_sub(first_alpha);
		alpha_list[1] = alpha_list[1].saturating_sub(second_alpha);
	}
	else {
		let start = (upper_left - left_index) as usize;
		if start < temporary_list.len() {
			compute_alpha_below_line(&mut temporary_list[start..], ul - (upper_left << 16), ll - (upper_left << 16), left_delta_y, full_alpha);
		}
		for index in upper_left..lower_left {
			let offset = (index - left_index) as usize;
			if offset < alpha_list.len() {
				alpha_list[offset] = alpha_list[offset].saturating_sub(temporary_list[offset]);
			}
		}
	}
	let upper_right = floor_fixed_to_int(ur);
	let lower_right = ceil_fixed_to_int(lr);
	if upper_right + 2 == lower_right {
		let first = (upper_right << 16) + FIXED_ONE - ur;
		let second = lr - ur - first;
		let first_alpha = partial_triangle_to_alpha(first, right_delta_y);
		let second_alpha = (full_alpha as i32 - partial_triangle_to_alpha(second, right_delta_y) as i32) as u8;
		let last = alpha_list.len();
		alpha_list[last - 2] = alpha_list[last - 2].saturating_sub(first_alpha);
		alpha_list[last - 1] = alpha_list[last - 1].saturating_sub(second_alpha);
	}
	else {
		let start = (upper_right - left_index) as usize;
		if start < temporary_list.len() {
			compute_alpha_above_line(&mut temporary_list[start..], ur - (upper_right << 16), lr - (upper_right << 16), right_delta_y, full_alpha);
		}
		for index in upper_right..lower_right {
			let offset = (index - left_index) as usize;
			if offset < alpha_list.len() {
				alpha_list[offset] = alpha_list[offset].saturating_sub(temporary_list[offset]);
			}
		}
	}
	if blitter.is_using_mask {
		for (offset, alpha) in alpha_list.iter().enumerate() {
			blitter.put_mask(left_index + offset as i32, row, *alpha, false);
		}
	}
	else if full_alpha == 0xFF && !is_no_real_blitter {
		for (offset, alpha) in alpha_list.iter().enumerate() {
			blitter.blend_direct(left_index + offset as i32, row, *alpha);
		}
	}
	else {
		blitter.add_row_alphas(left_index, row, &alpha_list);
	}
}

//==============================================================================
/// 사다리꼴 줄 얹기. (blit_trapezoid_row: 가운데 꽉 찬 몫은 한 번에, 양 끝은 삼각형으로)
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn blit_trapezoid_row(blitter: &mut CoverageBlitter, row: i32, mut ul: Fixed, mut ur: Fixed, mut ll: Fixed, mut lr: Fixed, left_delta_y: Fixed, right_delta_y: Fixed, full_alpha: u8, is_no_real_blitter: bool) {
	if ul > ur {
		return;
	}
	if ll > lr {
		ll = approximate_intersection(ul, ll, ur, lr);
		lr = ll;
	}
	if ul == ur && ll == lr {
		return;
	}
	if ul > ll {
		std::mem::swap(&mut ul, &mut ll);
	}
	if ur > lr {
		std::mem::swap(&mut ur, &mut lr);
	}
	let join_left = ceil_fixed_to_fixed(ll);
	let join_right = floor_fixed_to_fixed(ur);
	if join_left <= join_right {
		if ul < join_left {
			let length = ceil_fixed_to_int(join_left - ul);
			if length == 1 {
				let alpha = trapezoid_to_alpha(join_left - ul, join_left - ll);
				blit_single_alpha(blitter, row, ul >> 16, alpha, full_alpha, is_no_real_blitter);
			}
			else if length == 2 {
				let first = join_left - FIXED_ONE - ul;
				let second = ll - ul - first;
				let first_alpha = partial_triangle_to_alpha(first, left_delta_y);
				let second_alpha = (full_alpha as i32 - partial_triangle_to_alpha(second, left_delta_y) as i32) as u8;
				blit_two_alphas(blitter, row, ul >> 16, first_alpha, second_alpha, full_alpha, is_no_real_blitter);
			}
			else {
				blit_aaa_trapezoid_row(blitter, row, ul, join_left, ll, join_left, left_delta_y, i32::MAX, full_alpha, is_no_real_blitter);
			}
		}
		if join_left < join_right {
			blit_full_alpha(blitter, row, floor_fixed_to_int(join_left), floor_fixed_to_int(join_right - join_left), full_alpha, is_no_real_blitter);
		}
		if lr > join_right {
			let length = ceil_fixed_to_int(lr - join_right);
			if length == 1 {
				let alpha = trapezoid_to_alpha(ur - join_right, lr - join_right);
				blit_single_alpha(blitter, row, join_right >> 16, alpha, full_alpha, is_no_real_blitter);
			}
			else if length == 2 {
				let first = join_right + FIXED_ONE - ur;
				let second = lr - ur - first;
				let first_alpha = (full_alpha as i32 - partial_triangle_to_alpha(first, right_delta_y) as i32) as u8;
				let second_alpha = partial_triangle_to_alpha(second, right_delta_y);
				blit_two_alphas(blitter, row, join_right >> 16, first_alpha, second_alpha, full_alpha, is_no_real_blitter);
			}
			else {
				blit_aaa_trapezoid_row(blitter, row, join_right, ur, join_right, lr, i32::MAX, right_delta_y, full_alpha, is_no_real_blitter);
			}
		}
	}
	else {
		blit_aaa_trapezoid_row(blitter, row, ul, ur, ll, lr, left_delta_y, right_delta_y, full_alpha, is_no_real_blitter);
	}
}

//==============================================================================
// 가장자리 걷기. (aaa_walk_edges, 0 이 아닌 감김, 감싼 경로가 아니다)
//==============================================================================

//==============================================================================
/// 목록에서 빼기. (remove_edge)
//==============================================================================
fn remove_edge(edge_list: &mut [AnalyticEdge], edge: usize) {
	let (previous, next) = (edge_list[edge].previous, edge_list[edge].next);
	edge_list[previous].next = next;
	edge_list[next].previous = previous;
}

//==============================================================================
/// 목록에서 뒤에 넣기. (insert_edge_after)
//==============================================================================
fn insert_edge_after(edge_list: &mut [AnalyticEdge], edge: usize, after: usize) {
	let after_next = edge_list[after].next;
	edge_list[edge].previous = after;
	edge_list[edge].next = after_next;
	edge_list[after_next].previous = edge;
	edge_list[after].next = edge;
}

//==============================================================================
/// x 차례가 맞을 때까지 뒤로 옮기기. (backward_insert_edge_based_on_x)
//==============================================================================
fn insert_edge_backward_by_x(edge_list: &mut [AnalyticEdge], edge: usize) {
	let position_x = edge_list[edge].x;
	let mut previous = edge_list[edge].previous;
	while edge_list[previous].previous != NO_EDGE && edge_list[previous].x > position_x {
		previous = edge_list[previous].previous;
	}
	if edge_list[previous].next != edge {
		remove_edge(edge_list, edge);
		insert_edge_after(edge_list, edge, previous);
	}
}

//==============================================================================
/// 다음다음 y 고치기. (update_next_next_y)
//==============================================================================
fn update_next_next_y(position_y: Fixed, next_y: Fixed, next_next_y: &mut Fixed) {
	if position_y > next_y && position_y < *next_next_y {
		*next_next_y = position_y;
	}
}

//==============================================================================
/// 앞 가장자리와 다음 줄 안에서 엇갈리면 1/4 줄로 끊기. (check_intersection)
//==============================================================================
fn check_intersection(edge_list: &[AnalyticEdge], edge: usize, next_y: Fixed, next_next_y: &mut Fixed) {
	let previous = edge_list[edge].previous;
	if edge_list[previous].previous != NO_EDGE && edge_list[previous].x.wrapping_add(edge_list[previous].dx) > edge_list[edge].x.wrapping_add(edge_list[edge].dx) {
		*next_next_y = next_y + (FIXED_ONE >> DEFAULT_ACCURACY);
	}
}

//==============================================================================
/// 뒤 가장자리와 다음 줄 안에서 엇갈리면 1/4 줄로 끊기. (check_intersection_fwd)
//==============================================================================
fn check_intersection_forward(edge_list: &[AnalyticEdge], edge: usize, next_y: Fixed, next_next_y: &mut Fixed) {
	let next = edge_list[edge].next;
	if edge_list[next].next != NO_EDGE && edge_list[edge].x.wrapping_add(edge_list[edge].dx) > edge_list[next].x.wrapping_add(edge_list[next].dx) {
		*next_next_y = next_y + (FIXED_ONE >> DEFAULT_ACCURACY);
	}
}

//==============================================================================
/// 새로 시작하는 가장자리를 x 차례로 넣기. (insert_new_edges)
//==============================================================================
fn insert_new_edges(edge_list: &mut [AnalyticEdge], mut new_edge: usize, position_y: Fixed, next_next_y: &mut Fixed) {
	if edge_list[new_edge].upper_y > position_y {
		update_next_next_y(edge_list[new_edge].upper_y, position_y, next_next_y);
		return;
	}
	let previous = edge_list[new_edge].previous;
	if edge_list[previous].x <= edge_list[new_edge].x {
		while edge_list[new_edge].upper_y <= position_y {
			check_intersection(edge_list, new_edge, position_y, next_next_y);
			update_next_next_y(edge_list[new_edge].lower_y, position_y, next_next_y);
			new_edge = edge_list[new_edge].next;
		}
		update_next_next_y(edge_list[new_edge].upper_y, position_y, next_next_y);
		return;
	}
	let mut start = previous;
	while edge_list[start].previous != NO_EDGE && edge_list[start].x > edge_list[new_edge].x {
		start = edge_list[start].previous;
	}
	loop {
		let next = edge_list[new_edge].next;
		let mut is_in_place = false;
		loop {
			if edge_list[start].next == new_edge {
				is_in_place = true;
				break;
			}
			let after = edge_list[start].next;
			if edge_list[after].x >= edge_list[new_edge].x {
				break;
			}
			start = after;
		}
		if !is_in_place {
			remove_edge(edge_list, new_edge);
			insert_edge_after(edge_list, new_edge, start);
		}
		check_intersection(edge_list, new_edge, position_y, next_next_y);
		check_intersection_forward(edge_list, new_edge, position_y, next_next_y);
		update_next_next_y(edge_list[new_edge].lower_y, position_y, next_next_y);
		start = new_edge;
		new_edge = next;
		if edge_list[new_edge].upper_y > position_y {
			break;
		}
	}
	update_next_next_y(edge_list[new_edge].upper_y, position_y, next_next_y);
}

//==============================================================================
/// 두 가장자리가 이 줄 안에서 가까운지. (edges_too_close)
//==============================================================================
fn are_edges_too_close(edge_list: &[AnalyticEdge], previous: usize, next: usize, lower_y: Fixed) -> bool {
	if previous == NO_EDGE || next == NO_EDGE {
		return false;
	}
	edge_list[next].upper_y < lower_y && edge_list[previous].x.wrapping_add(FIXED_ONE) >= edge_list[next].x.wrapping_sub(edge_list[next].dx.wrapping_abs())
}

//==============================================================================
/// 앞 오른쪽 가장자리가 빠진 뒤 가까운지. (edges_too_close 의 정수 갈래)
//==============================================================================
fn is_previous_right_too_close(previous_right: i32, ul: Fixed, ll: Fixed) -> bool {
	previous_right > floor_fixed_to_int(ul) || previous_right > floor_fixed_to_int(ll)
}

//==============================================================================
/// 가장자리를 줄마다 걸으며 칠하기. (aaa_walk_edges, 0 이 아닌 감김)
//==============================================================================
fn walk_edges(edge_list: &mut [AnalyticEdge], blitter: &mut CoverageBlitter, start_y: i32, stop_y: i32, left_clip: Fixed, right_clip: Fixed, is_intersect_skipped: bool) {
	edge_list[HEAD_EDGE].x = left_clip;
	edge_list[HEAD_EDGE].upper_x = left_clip;
	edge_list[TAIL_EDGE].x = right_clip;
	edge_list[TAIL_EDGE].upper_x = right_clip;
	let mut position_y = edge_list[edge_list[HEAD_EDGE].next].upper_y.max(start_y << 16);
	let mut next_next_y = i32::MAX;
	{
		let mut edge = edge_list[HEAD_EDGE].next;
		while edge_list[edge].upper_y <= position_y {
			edge_list[edge].go_y(position_y);
			update_next_next_y(edge_list[edge].lower_y, position_y, &mut next_next_y);
			edge = edge_list[edge].next;
		}
		update_next_next_y(edge_list[edge].upper_y, position_y, &mut next_next_y);
	}
	let winding_mask = -1;
	loop {
		let mut winding = 0;
		let mut is_in_interval = false;
		let mut previous_x = edge_list[HEAD_EDGE].x;
		let mut next_y = next_next_y.min(ceil_fixed_to_fixed(position_y.wrapping_add(1)));
		let mut current_edge = edge_list[HEAD_EDGE].next;
		let mut left_edge = HEAD_EDGE;
		let mut left = left_clip;
		let mut left_delta_y = 0;
		let mut previous_right = floor_fixed_to_int(left_clip);
		next_next_y = i32::MAX;
		let mut y_shift = 0;
		if (next_y - position_y) & (FIXED_ONE >> 2) != 0 {
			y_shift = 2;
			next_y = position_y + (FIXED_ONE >> 2);
		}
		else if (next_y - position_y) & (FIXED_ONE >> 1) != 0 {
			y_shift = 1;
		}
		let full_alpha = fixed_to_alpha(next_y - position_y);
		while edge_list[current_edge].upper_y <= position_y {
			winding += edge_list[current_edge].winding;
			let was_in_interval = is_in_interval;
			is_in_interval = (winding & winding_mask) != 0;
			let is_left = is_in_interval && !was_in_interval;
			let is_right = !is_in_interval && was_in_interval;
			if is_right {
				let mut right = edge_list[current_edge].x;
				edge_list[current_edge].go_y_shifted(next_y, y_shift);
				let next_left = left_clip.max(edge_list[left_edge].x);
				right = right_clip.min(right);
				let next_right = right_clip.min(edge_list[current_edge].x);
				let is_too_close = full_alpha == 0xFF && (is_previous_right_too_close(previous_right, left, edge_list[left_edge].x) || are_edges_too_close(edge_list, current_edge, edge_list[current_edge].next, next_y));
				blit_trapezoid_row(blitter, position_y >> 16, left, right, next_left, next_right, left_delta_y, edge_list[current_edge].dy, full_alpha, is_too_close);
				previous_right = ceil_fixed_to_int(right.max(edge_list[current_edge].x));
			}
			else {
				if is_left {
					left = edge_list[current_edge].x.max(left_clip);
					left_delta_y = edge_list[current_edge].dy;
					left_edge = current_edge;
				}
				edge_list[current_edge].go_y_shifted(next_y, y_shift);
			}
			let next = edge_list[current_edge].next;
			while edge_list[current_edge].lower_y <= next_y {
				let curve_count = edge_list[current_edge].curve_count;
				if curve_count < 0 {
					edge_list[current_edge].keep_continuous();
					if !edge_list[current_edge].update_cubic() {
						break;
					}
				}
				else if curve_count > 0 {
					edge_list[current_edge].keep_continuous();
					if !edge_list[current_edge].update_quadratic() {
						break;
					}
				}
				else {
					break;
				}
			}
			if edge_list[current_edge].lower_y <= next_y {
				remove_edge(edge_list, current_edge);
			}
			else {
				update_next_next_y(edge_list[current_edge].lower_y, next_y, &mut next_next_y);
				let new_x = edge_list[current_edge].x;
				if new_x < previous_x {
					insert_edge_backward_by_x(edge_list, current_edge);
				}
				else {
					previous_x = new_x;
				}
				if !is_intersect_skipped {
					check_intersection(edge_list, current_edge, next_y, &mut next_next_y);
				}
			}
			current_edge = next;
		}
		if is_in_interval {
			let is_too_close = full_alpha == 0xFF && are_edges_too_close(edge_list, edge_list[left_edge].previous, left_edge, next_y);
			blit_trapezoid_row(blitter, position_y >> 16, left, right_clip, left_clip.max(edge_list[left_edge].x), right_clip, left_delta_y, 0, full_alpha, is_too_close);
		}
		position_y = next_y;
		if position_y >= stop_y << 16 {
			break;
		}
		insert_new_edges(edge_list, current_edge, position_y, &mut next_next_y);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 정수 자리 사각형은 안이 255, 밖이 0 이다.
	//==============================================================================
	#[test]
	fn rectangle_covers_inside() {
		let mut path = DevicePath::new();
		path.move_to([1.0, 1.0]);
		path.line_to([5.0, 1.0]);
		path.line_to([5.0, 4.0]);
		path.line_to([1.0, 4.0]);
		path.close();
		let (coverage, width, height, left_x, top_y) = rasterize_path(&path).expect("raster");
		assert_eq!((width, height, left_x, top_y), (4, 3, 1, 1));
		assert!(coverage.iter().all(|value| *value == 255));
	}

	//==============================================================================
	/// 반 픽셀 걸친 가장자리는 절반쯤 덮인다. (마스크 갈래와 RLE 갈래 둘 다)
	//==============================================================================
	#[test]
	fn half_pixel_edge_is_half_covered() {
		for right in [4.5f32, 40.5] {
			let mut path = DevicePath::new();
			path.move_to([0.0, 0.0]);
			path.line_to([right, 0.0]);
			path.line_to([right, 8.0]);
			path.line_to([0.0, 8.0]);
			path.close();
			let (coverage, width, height, _, _) = rasterize_path(&path).expect("raster");
			let last_column = (width - 1) as usize;
			for row in 0..height as usize {
				let value = coverage[row * width as usize + last_column];
				assert!((126..=129).contains(&value), "{value}");
				assert_eq!(coverage[row * width as usize], 255);
			}
		}
	}

	//==============================================================================
	/// 겹친 두 윤곽은 0 이 아닌 감김으로 합쳐 255 를 넘지 않는다.
	//==============================================================================
	#[test]
	fn overlapping_contours_follow_nonzero() {
		let mut path = DevicePath::new();
		for offset in [0.0f32, 0.5] {
			path.move_to([offset, 0.0]);
			path.line_to([offset + 3.0, 0.0]);
			path.line_to([offset + 3.0, 4.0]);
			path.line_to([offset, 4.0]);
			path.close();
		}
		let (coverage, width, _, _, _) = rasterize_path(&path).expect("raster");
		assert_eq!(width, 4);
		assert_eq!(coverage[1], 255);
		assert!(coverage[3] >= 126 && coverage[3] <= 129);
	}

	//==============================================================================
	/// 빠른 역수는 스키아 표처럼 2^22 / x 를 0 쪽으로 자른다.
	//==============================================================================
	#[test]
	fn quick_inverse_matches_table() {
		assert_eq!(quick_inverse(1), 4194304);
		assert_eq!(quick_inverse(3), 1398101);
		assert_eq!(quick_inverse(-1000), -4194);
		assert_eq!(quick_inverse(1024), 4096);
		assert_eq!(quick_inverse(0), 0);
	}
}

