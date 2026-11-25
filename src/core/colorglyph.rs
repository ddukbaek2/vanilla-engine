//==============================================================================
// 색 글리프 래스터. (이모지 글꼴의 색 글리프를 프리멀티플라이드 RGBA 로)
// - 원본은 브라우저 Canvas2D 가 색 글리프를 제 색으로 그린다. fillStyle 의 색은 먹지 않고 알파만 먹으며, strokeText 도 색 글리프는 테를 긋지 않고 색 그대로 칠한다.
//   러스트 판은 글꼴의 색 글리프 자료를 직접 읽어 그린다. 크롬처럼 OT-SVG 글리프는 그리지 않는다.
// - 비트맵: sbix (애플 컬러 이모지), CBDT/CBLC (노토 컬러 이모지 비트맵 판)의 PNG 와 프리멀티플라이드 BGRA 를 꺼내 풀고 글자 크기에 맞춘다.
//   판은 그 글리프를 가진 판 가운데 글자 크기 이상인 가장 작은 것, 없으면 가장 큰 것이다. (애플 컬러 이모지는 판마다 빠진 글리프가 있다) 줄일 때는 넓이 평균, 키울 때는 두 점 사이 섞기다.
//   자리는 판의 픽셀 단위 자리 값(그림의 왼쪽 아래가 펜 원점에서 떨어진 거리)을 글자 크기 비로 옮긴 것이다.
// - 윤곽: COLR/CPAL v0 (층마다 윤곽 하나를 한 색으로)과 v1 (그라데이션, 변환, 자르기, 합성). ttf-parser 의 Painter 로 받아 직접 칠한다.
//   윤곽은 곡선을 선분으로 펴서 넓이 누적 방식(font-rs 와 같은 방식, 0 이 아닌 감김)으로 덮인 정도를 셈한다.
//   칠은 한 색, 선형(세 점), 두 원 사이 방사, 스윕 그라데이션을 픽셀 가운데에서 셈한다. 색 줄의 끝 밖은 Pad, Repeat, Reflect 를 따르고 색은 곧은 알파로 섞는다.
//   합성은 COLR 의 28 가지(Porter-Duff 와 W3C 섞기)를 프리멀티플라이드 색으로 한다. 팔레트는 0 번, 글자 색(팔레트 번호 0xFFFF)은 흰색이다. (글자는 흰색으로 굽고 그릴 때 색을 곱한다)
//   v1 의 칠(paint)은 지금 자르기 영역을 칠한다. 자르기가 없으면(v0) 마지막으로 받은 윤곽을 칠한다.
// - ttf-parser 는 rustybuzz 가 다시 내보내는 것을 쓴다. (모양 짓기 얼굴과 같은 얼굴)
//==============================================================================
use rustybuzz::ttf_parser::colr::{ClipBox, CompositeMode, GradientExtend, Paint, Painter};
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder, RasterGlyphImage, RasterImageFormat, RgbaColor, Transform};

// 그릴 수 있는 색 글리프의 가로세로 상한. (픽셀, 이상한 글꼴 자료로 큰 판을 잡지 않게)
const MAXIMUM_RASTER_SIZE: i32 = 4096;

// 글자 색. (원본은 흰색으로 굽는다)
const FOREGROUND_COLOR: RgbaColor = RgbaColor { red: 255, green: 255, blue: 255, alpha: 255 };

//==============================================================================
// 색 글리프 래스터 하나. (프리멀티플라이드 RGBA, 왼쪽 위 자리는 펜 원점에서 오른쪽 left_x, 기준선에서 위로 top_y)
//==============================================================================
pub struct ColorGlyphRaster {
	pub pixels: Vec<u8>,
	pub width: i32,
	pub height: i32,
	pub left_x: i32,
	pub top_y: i32,
}

//==============================================================================
// 글꼴에 색 글리프 표가 있는지. (sbix, CBDT, COLR)
//==============================================================================
pub fn has_color_tables(face: &Face) -> bool {
	let tables = face.tables();
	tables.sbix.is_some() || tables.cbdt.is_some() || tables.colr.is_some()
}

//==============================================================================
// 글리프가 색 글리프인지. (COLR 에 있거나, PNG / BGRA 비트맵이 있다)
//==============================================================================
pub fn has_color_glyph(face: &Face, glyph_index: u16) -> bool {
	if face.is_color_glyph(GlyphId(glyph_index)) {
		return true;
	}
	match read_raster_image(face, glyph_index, 64) {
		Some(raster_image) => {
			matches!(raster_image.format, RasterImageFormat::PNG | RasterImageFormat::BitmapPremulBgra32)
		}
		None => {
			false
		}
	}
}

//==============================================================================
// 색 글리프 래스터. (COLR 이 먼저, 없으면 비트맵, 색 글리프가 아니면 None)
//==============================================================================
pub fn rasterize_color_glyph(face: &Face<'static>, glyph_index: u16, pixel_size: f32) -> Option<ColorGlyphRaster> {
	if pixel_size <= 0.0 {
		return None;
	}
	if face.is_color_glyph(GlyphId(glyph_index)) {
		if let Some(color_glyph_raster) = rasterize_outline_color_glyph(face, glyph_index, pixel_size) {
			return Some(color_glyph_raster);
		}
	}
	rasterize_bitmap_color_glyph(face, glyph_index, pixel_size)
}

//==============================================================================
// 글리프의 비트맵 그림 찾기. (sbix 는 그 글리프를 가진 판 가운데 글자 크기 이상인 가장 작은 것, 없으면 가장 큰 것. 그 밖은 ttf-parser 가 고른 판)
//==============================================================================
fn read_raster_image<'a>(face: &'a Face, glyph_index: u16, pixels_per_em: u16) -> Option<RasterGlyphImage<'a>> {
	let sbix_table = match face.tables().sbix {
		Some(sbix_table) => {
			sbix_table
		}
		None => {
			return face.glyph_raster_image(GlyphId(glyph_index), pixels_per_em);
		}
	};
	let mut best_image: Option<RasterGlyphImage<'a>> = None;
	for strike in sbix_table.strikes {
		let raster_image = match strike.get(GlyphId(glyph_index)) {
			Some(raster_image) => {
				raster_image
			}
			None => {
				continue;
			}
		};
		let is_better = match best_image.as_ref() {
			Some(current_image) => {
				if current_image.pixels_per_em >= pixels_per_em {
					raster_image.pixels_per_em >= pixels_per_em && raster_image.pixels_per_em < current_image.pixels_per_em
				}
				else {
					raster_image.pixels_per_em > current_image.pixels_per_em
				}
			}
			None => {
				true
			}
		};
		if is_better {
			best_image = Some(raster_image);
		}
	}
	best_image
}

//==============================================================================
// 비트맵 색 글리프 래스터. (sbix, CBDT 의 그림을 풀어 글자 크기로)
//==============================================================================
fn rasterize_bitmap_color_glyph(face: &Face, glyph_index: u16, pixel_size: f32) -> Option<ColorGlyphRaster> {
	let pixels_per_em = (pixel_size.ceil() as u32).clamp(1, u16::MAX as u32) as u16;
	let raster_image = read_raster_image(face, glyph_index, pixels_per_em)?;
	let (source_pixels, source_width, source_height) = match raster_image.format {
		RasterImageFormat::PNG => {
			let decoded_image = ::image::load_from_memory_with_format(raster_image.data, ::image::ImageFormat::Png).ok()?.to_rgba8();
			let (decoded_width, decoded_height) = decoded_image.dimensions();
			let mut pixels = decoded_image.into_raw();
			for pixel in pixels.chunks_exact_mut(4) {
				let alpha = pixel[3] as u32;
				for channel_index in 0..3 {
					pixel[channel_index] = ((pixel[channel_index] as u32 * alpha + 127) / 255) as u8;
				}
			}
			(pixels, decoded_width as usize, decoded_height as usize)
		}
		RasterImageFormat::BitmapPremulBgra32 => {
			let width = raster_image.width as usize;
			let height = raster_image.height as usize;
			if raster_image.data.len() < width * height * 4 {
				return None;
			}
			let mut pixels = raster_image.data[..width * height * 4].to_vec();
			for pixel in pixels.chunks_exact_mut(4) {
				pixel.swap(0, 2);
			}
			(pixels, width, height)
		}
		_ => {
			return None;
		}
	};
	if source_width == 0 || source_height == 0 {
		return None;
	}
	// 판 픽셀 → 글자 크기 픽셀. (그림의 왼쪽 아래가 펜 원점에서 (x, y), y 는 위가 양수)
	let scale = pixel_size / raster_image.pixels_per_em.max(1) as f32;
	let left = raster_image.x as f32 * scale;
	let right = (raster_image.x as f32 + source_width as f32) * scale;
	let bottom = raster_image.y as f32 * scale;
	let top = (raster_image.y as f32 + source_height as f32) * scale;
	let left_x = left.floor() as i32;
	let top_y = top.ceil() as i32;
	let width = right.ceil() as i32 - left_x;
	let height = top_y - bottom.floor() as i32;
	if width <= 0 || height <= 0 || width > MAXIMUM_RASTER_SIZE || height > MAXIMUM_RASTER_SIZE {
		return None;
	}
	let column_weight_list = build_resample_weights(source_width, left - left_x as f32, scale, width as usize);
	let row_weight_list = build_resample_weights(source_height, top_y as f32 - top, scale, height as usize);
	// 가로 먼저, 다음 세로. (프리멀티플라이드 색이라 그대로 섞는다)
	let mut horizontal_pixels = vec![0.0f32; source_height * width as usize * 4];
	for source_y in 0..source_height {
		for (destination_x, weight_list) in column_weight_list.iter().enumerate() {
			let mut color = [0.0f32; 4];
			for (source_x, weight) in weight_list {
				let source_index = (source_y * source_width + source_x) * 4;
				for channel_index in 0..4 {
					color[channel_index] += source_pixels[source_index + channel_index] as f32 * weight;
				}
			}
			let destination_index = (source_y * width as usize + destination_x) * 4;
			horizontal_pixels[destination_index..destination_index + 4].copy_from_slice(&color);
		}
	}
	let mut pixels = vec![0u8; (width * height * 4) as usize];
	for (destination_y, weight_list) in row_weight_list.iter().enumerate() {
		for destination_x in 0..width as usize {
			let mut color = [0.0f32; 4];
			for (source_y, weight) in weight_list {
				let source_index = (source_y * width as usize + destination_x) * 4;
				for channel_index in 0..4 {
					color[channel_index] += horizontal_pixels[source_index + channel_index] * weight;
				}
			}
			let destination_index = (destination_y * width as usize + destination_x) * 4;
			let alpha = color[3].round().clamp(0.0, 255.0);
			pixels[destination_index + 3] = alpha as u8;
			for channel_index in 0..3 {
				pixels[destination_index + channel_index] = color[channel_index].round().clamp(0.0, alpha) as u8;
			}
		}
	}
	Some(ColorGlyphRaster {
		pixels,
		width,
		height,
		left_x,
		top_y,
	})
}

//==============================================================================
// 한 축의 다시 뽑기 무게. (원본 픽셀 j 는 받는 쪽 [offset + j × scale, offset + (j + 1) × scale) 을 덮는다)
// - 줄일 때(scale <= 1)는 받는 픽셀과 겹친 길이, 키울 때는 받는 픽셀 가운데의 두 점 사이 섞기다. 그림 밖은 투명으로 본다.
//==============================================================================
fn build_resample_weights(source_length: usize, offset: f32, scale: f32, destination_length: usize) -> Vec<Vec<(usize, f32)>> {
	let mut weight_table: Vec<Vec<(usize, f32)>> = Vec::with_capacity(destination_length);
	for destination_index in 0..destination_length {
		let mut weight_list: Vec<(usize, f32)> = Vec::new();
		if scale <= 1.0 {
			let destination_start = destination_index as f32;
			let destination_end = destination_start + 1.0;
			let first_source = (((destination_start - offset) / scale).floor().max(0.0)) as usize;
			let last_source = (((destination_end - offset) / scale).ceil().max(0.0) as usize).min(source_length);
			for source_index in first_source..last_source {
				let source_start = offset + source_index as f32 * scale;
				let source_end = source_start + scale;
				let overlap = destination_end.min(source_end) - destination_start.max(source_start);
				if overlap > 0.0 {
					weight_list.push((source_index, overlap));
				}
			}
		}
		else {
			let source_position = (destination_index as f32 + 0.5 - offset) / scale - 0.5;
			let source_floor = source_position.floor();
			let fraction = source_position - source_floor;
			for (source_index, weight) in [(source_floor as i64, 1.0 - fraction), (source_floor as i64 + 1, fraction)] {
				if source_index >= 0 && (source_index as usize) < source_length && weight > 0.0 {
					weight_list.push((source_index as usize, weight));
				}
			}
		}
		weight_table.push(weight_list);
	}
	weight_table
}

//==============================================================================
// COLR 색 글리프 래스터. (경계를 먼저 재고, 그 판에 칠한다)
//==============================================================================
fn rasterize_outline_color_glyph(face: &Face<'static>, glyph_index: u16, pixel_size: f32) -> Option<ColorGlyphRaster> {
	let scale = pixel_size / face.units_per_em() as f32;
	let mut bounds_painter = BoundsPainter {
		face,
		transform_stack: Vec::new(),
		transform: Transform::default(),
		outline_bounds: None,
		clip_bounds: None,
	};
	face.paint_color_glyph(GlyphId(glyph_index), 0, FOREGROUND_COLOR, &mut bounds_painter)?;
	let bounds = bounds_painter.outline_bounds.or(bounds_painter.clip_bounds)?;
	let left_x = (bounds[0] * scale).floor() as i32 - 1;
	let right_x = (bounds[2] * scale).ceil() as i32 + 1;
	let bottom_y = (bounds[1] * scale).floor() as i32 - 1;
	let top_y = (bounds[3] * scale).ceil() as i32 + 1;
	let width = right_x - left_x;
	let height = top_y - bottom_y;
	if width <= 0 || height <= 0 || width > MAXIMUM_RASTER_SIZE || height > MAXIMUM_RASTER_SIZE {
		return None;
	}
	let pixel_count = (width * height) as usize;
	let mut color_painter = ColorPainter {
		face,
		width,
		height,
		scale,
		left_x: left_x as f32,
		top_y: top_y as f32,
		transform_stack: Vec::new(),
		transform: Transform::default(),
		outline_segment_list: Vec::new(),
		clip_stack: Vec::new(),
		layer_stack: Vec::new(),
		canvas: vec![[0.0; 4]; pixel_count],
	};
	face.paint_color_glyph(GlyphId(glyph_index), 0, FOREGROUND_COLOR, &mut color_painter)?;
	let mut pixels = vec![0u8; pixel_count * 4];
	for (pixel_index, color) in color_painter.canvas.iter().enumerate() {
		let alpha = color[3].clamp(0.0, 1.0);
		pixels[pixel_index * 4 + 3] = (alpha * 255.0).round() as u8;
		for channel_index in 0..3 {
			pixels[pixel_index * 4 + channel_index] = (color[channel_index].clamp(0.0, alpha) * 255.0).round() as u8;
		}
	}
	Some(ColorGlyphRaster {
		pixels,
		width,
		height,
		left_x,
		top_y,
	})
}

//==============================================================================
// 점 하나를 변환으로 옮기기.
//==============================================================================
fn apply_transform(transform: &Transform, position_x: f32, position_y: f32) -> (f32, f32) {
	(transform.a * position_x + transform.c * position_y + transform.e, transform.b * position_x + transform.d * position_y + transform.f)
}

//==============================================================================
// 경계 재는 그리개. (변환을 건 윤곽 경계 상자들의 합, 윤곽이 없으면 자르기 상자들의 합. 글꼴 단위)
//==============================================================================
struct BoundsPainter<'face> {
	face: &'face Face<'static>,
	transform_stack: Vec<Transform>,
	transform: Transform,
	outline_bounds: Option<[f32; 4]>,
	clip_bounds: Option<[f32; 4]>,
}

//==============================================================================
// 상자 네 모서리를 변환해 경계에 더하기.
//==============================================================================
fn add_transformed_box(bounds: &mut Option<[f32; 4]>, transform: &Transform, box_bounds: [f32; 4]) {
	for (corner_x, corner_y) in [(box_bounds[0], box_bounds[1]), (box_bounds[2], box_bounds[1]), (box_bounds[0], box_bounds[3]), (box_bounds[2], box_bounds[3])] {
		let (position_x, position_y) = apply_transform(transform, corner_x, corner_y);
		match bounds {
			Some(existing_bounds) => {
				existing_bounds[0] = existing_bounds[0].min(position_x);
				existing_bounds[1] = existing_bounds[1].min(position_y);
				existing_bounds[2] = existing_bounds[2].max(position_x);
				existing_bounds[3] = existing_bounds[3].max(position_y);
			}
			None => {
				*bounds = Some([position_x, position_y, position_x, position_y]);
			}
		}
	}
}

impl<'face> Painter<'static> for BoundsPainter<'face> {
	//==============================================================================
	// 윤곽 받기. (경계 상자만)
	//==============================================================================
	fn outline_glyph(&mut self, glyph_id: GlyphId) {
		let mut segment_collector = SegmentCollector::new(Transform::default());
		if let Some(bounding_box) = self.face.outline_glyph(glyph_id, &mut segment_collector) {
			add_transformed_box(&mut self.outline_bounds, &self.transform, [bounding_box.x_min as f32, bounding_box.y_min as f32, bounding_box.x_max as f32, bounding_box.y_max as f32]);
		}
	}

	//==============================================================================
	// 칠. (경계에는 쓰지 않는다)
	//==============================================================================
	fn paint(&mut self, _paint: Paint<'static>) {}

	//==============================================================================
	// 윤곽 자르기. (경계에는 쓰지 않는다)
	//==============================================================================
	fn push_clip(&mut self) {}

	//==============================================================================
	// 자르기 상자. (윤곽이 하나도 없을 때의 경계)
	//==============================================================================
	fn push_clip_box(&mut self, clip_box: ClipBox) {
		add_transformed_box(&mut self.clip_bounds, &self.transform, [clip_box.x_min, clip_box.y_min, clip_box.x_max, clip_box.y_max]);
	}

	//==============================================================================
	// 자르기 빼기. (경계에는 쓰지 않는다)
	//==============================================================================
	fn pop_clip(&mut self) {}

	//==============================================================================
	// 층 쌓기. (경계에는 쓰지 않는다)
	//==============================================================================
	fn push_layer(&mut self, _mode: CompositeMode) {}

	//==============================================================================
	// 층 빼기. (경계에는 쓰지 않는다)
	//==============================================================================
	fn pop_layer(&mut self) {}

	//==============================================================================
	// 변환 쌓기.
	//==============================================================================
	fn push_transform(&mut self, transform: Transform) {
		self.transform_stack.push(self.transform);
		self.transform = Transform::combine(self.transform, transform);
	}

	//==============================================================================
	// 변환 빼기.
	//==============================================================================
	fn pop_transform(&mut self) {
		if let Some(transform) = self.transform_stack.pop() {
			self.transform = transform;
		}
	}
}

//==============================================================================
// 윤곽을 선분으로 모으기. (곡선은 펴고, 점은 변환으로 옮긴다)
//==============================================================================
struct SegmentCollector {
	transform: Transform,
	segment_list: Vec<[f32; 4]>,
	current_point: (f32, f32),
	contour_start_point: (f32, f32),
}

impl SegmentCollector {
	//==============================================================================
	// 생성. (변환은 글꼴 단위 → 받을 좌표)
	//==============================================================================
	fn new(transform: Transform) -> SegmentCollector {
		SegmentCollector {
			transform,
			segment_list: Vec::new(),
			current_point: (0.0, 0.0),
			contour_start_point: (0.0, 0.0),
		}
	}

	//==============================================================================
	// 옮긴 점까지 선분 하나.
	//==============================================================================
	fn push_line(&mut self, end_point: (f32, f32)) {
		self.segment_list.push([self.current_point.0, self.current_point.1, end_point.0, end_point.1]);
		self.current_point = end_point;
	}

	//==============================================================================
	// 곡선을 몇 조각으로 펼지. (font-rs 처럼 굽은 정도의 제곱근의 제곱근)
	//==============================================================================
	fn count_curve_pieces(deviation_squared: f32) -> usize {
		if deviation_squared < 0.333 {
			return 1;
		}
		1 + (3.0 * deviation_squared).sqrt().sqrt().floor() as usize
	}
}

impl OutlineBuilder for SegmentCollector {
	//==============================================================================
	// 새 윤곽 시작.
	//==============================================================================
	fn move_to(&mut self, position_x: f32, position_y: f32) {
		self.current_point = apply_transform(&self.transform, position_x, position_y);
		self.contour_start_point = self.current_point;
	}

	//==============================================================================
	// 선분.
	//==============================================================================
	fn line_to(&mut self, position_x: f32, position_y: f32) {
		let end_point = apply_transform(&self.transform, position_x, position_y);
		self.push_line(end_point);
	}

	//==============================================================================
	// 2 차 곡선. (옮긴 뒤의 점으로 편다)
	//==============================================================================
	fn quad_to(&mut self, control_x: f32, control_y: f32, position_x: f32, position_y: f32) {
		let start_point = self.current_point;
		let control_point = apply_transform(&self.transform, control_x, control_y);
		let end_point = apply_transform(&self.transform, position_x, position_y);
		let deviation_x = start_point.0 - 2.0 * control_point.0 + end_point.0;
		let deviation_y = start_point.1 - 2.0 * control_point.1 + end_point.1;
		let piece_count = SegmentCollector::count_curve_pieces(deviation_x * deviation_x + deviation_y * deviation_y);
		for piece_index in 1..=piece_count {
			let t = piece_index as f32 / piece_count as f32;
			let inverse_t = 1.0 - t;
			let point_x = inverse_t * inverse_t * start_point.0 + 2.0 * inverse_t * t * control_point.0 + t * t * end_point.0;
			let point_y = inverse_t * inverse_t * start_point.1 + 2.0 * inverse_t * t * control_point.1 + t * t * end_point.1;
			self.push_line((point_x, point_y));
		}
	}

	//==============================================================================
	// 3 차 곡선. (옮긴 뒤의 점으로 편다)
	//==============================================================================
	fn curve_to(&mut self, first_control_x: f32, first_control_y: f32, second_control_x: f32, second_control_y: f32, position_x: f32, position_y: f32) {
		let start_point = self.current_point;
		let first_control_point = apply_transform(&self.transform, first_control_x, first_control_y);
		let second_control_point = apply_transform(&self.transform, second_control_x, second_control_y);
		let end_point = apply_transform(&self.transform, position_x, position_y);
		let first_deviation_x = start_point.0 - 2.0 * first_control_point.0 + second_control_point.0;
		let first_deviation_y = start_point.1 - 2.0 * first_control_point.1 + second_control_point.1;
		let second_deviation_x = first_control_point.0 - 2.0 * second_control_point.0 + end_point.0;
		let second_deviation_y = first_control_point.1 - 2.0 * second_control_point.1 + end_point.1;
		let deviation_squared = (first_deviation_x * first_deviation_x + first_deviation_y * first_deviation_y).max(second_deviation_x * second_deviation_x + second_deviation_y * second_deviation_y);
		let piece_count = SegmentCollector::count_curve_pieces(deviation_squared);
		for piece_index in 1..=piece_count {
			let t = piece_index as f32 / piece_count as f32;
			let inverse_t = 1.0 - t;
			let first_weight = inverse_t * inverse_t * inverse_t;
			let second_weight = 3.0 * inverse_t * inverse_t * t;
			let third_weight = 3.0 * inverse_t * t * t;
			let fourth_weight = t * t * t;
			let point_x = first_weight * start_point.0 + second_weight * first_control_point.0 + third_weight * second_control_point.0 + fourth_weight * end_point.0;
			let point_y = first_weight * start_point.1 + second_weight * first_control_point.1 + third_weight * second_control_point.1 + fourth_weight * end_point.1;
			self.push_line((point_x, point_y));
		}
	}

	//==============================================================================
	// 윤곽 닫기. (시작점으로)
	//==============================================================================
	fn close(&mut self) {
		let contour_start_point = self.contour_start_point;
		if contour_start_point != self.current_point {
			self.push_line(contour_start_point);
		}
	}
}

//==============================================================================
// 선분들의 덮인 정도. (넓이 누적, 0 이 아닌 감김에 가깝게 절댓값을 1 에서 자른다. 선분은 픽셀 좌표, y 는 아래로)
//==============================================================================
fn rasterize_segments(segment_list: &[[f32; 4]], width: i32, height: i32) -> Vec<f32> {
	// 한 줄 끝에 셋을 더 두어 판 오른쪽 밖으로 민 값을 받는다. (줄마다 따로 더한다)
	let stride = width as usize + 3;
	let mut accumulation = vec![0.0f32; stride * height as usize];
	for segment in segment_list {
		accumulate_line(&mut accumulation, stride, width, height, segment);
	}
	let mut coverage = vec![0.0f32; (width * height) as usize];
	for row_index in 0..height as usize {
		let mut sum = 0.0f32;
		for column_index in 0..width as usize {
			sum += accumulation[row_index * stride + column_index];
			coverage[row_index * width as usize + column_index] = sum.abs().min(1.0);
		}
	}
	coverage
}

//==============================================================================
// 선분 하나를 넓이로 쌓기. (font-rs 의 draw_line, 판 왼쪽과 오른쪽 밖은 가장자리로 민다)
//==============================================================================
fn accumulate_line(accumulation: &mut [f32], stride: usize, width: i32, height: i32, segment: &[f32; 4]) {
	let (start_x, start_y, end_x, end_y) = (segment[0], segment[1], segment[2], segment[3]);
	if (start_y - end_y).abs() <= f32::EPSILON || !start_y.is_finite() || !end_y.is_finite() || !start_x.is_finite() || !end_x.is_finite() {
		return;
	}
	let (direction, top_x, top_y, bottom_y, bottom_x) = if start_y < end_y {
		(1.0f32, start_x, start_y, end_y, end_x)
	}
	else {
		(-1.0f32, end_x, end_y, start_y, start_x)
	};
	let slope = (bottom_x - top_x) / (bottom_y - top_y);
	let mut position_x = top_x;
	if top_y < 0.0 {
		position_x -= top_y * slope;
	}
	let first_row = top_y.max(0.0) as usize;
	let last_row = (bottom_y.ceil().max(0.0) as usize).min(height as usize);
	let width_limit = width as f32;
	for row_index in first_row..last_row {
		let row_start = row_index * stride;
		let row_height = ((row_index + 1) as f32).min(bottom_y) - (row_index as f32).max(top_y);
		let next_position_x = position_x + slope * row_height;
		let delta = row_height * direction;
		let clamped_x = position_x.clamp(0.0, width_limit);
		let clamped_next_x = next_position_x.clamp(0.0, width_limit);
		let (left, right) = if clamped_x < clamped_next_x {
			(clamped_x, clamped_next_x)
		}
		else {
			(clamped_next_x, clamped_x)
		};
		let left_floor = left.floor();
		let left_index = left_floor as usize;
		let right_ceiling = right.ceil();
		let right_index = right_ceiling as usize;
		if right_index <= left_index + 1 {
			let middle_fraction = 0.5 * (clamped_x + clamped_next_x) - left_floor;
			accumulation[row_start + left_index] += delta - delta * middle_fraction;
			accumulation[row_start + left_index + 1] += delta * middle_fraction;
		}
		else {
			let inverse_span = (right - left).recip();
			let left_fraction = left - left_floor;
			let first_area = 0.5 * inverse_span * (1.0 - left_fraction) * (1.0 - left_fraction);
			let right_fraction = right - right_ceiling + 1.0;
			let last_area = 0.5 * inverse_span * right_fraction * right_fraction;
			accumulation[row_start + left_index] += delta * first_area;
			if right_index == left_index + 2 {
				accumulation[row_start + left_index + 1] += delta * (1.0 - first_area - last_area);
			}
			else {
				let second_area = inverse_span * (1.5 - left_fraction);
				accumulation[row_start + left_index + 1] += delta * (second_area - first_area);
				for column_index in left_index + 2..right_index - 1 {
					accumulation[row_start + column_index] += delta * inverse_span;
				}
				let before_last_area = second_area + (right_index - left_index - 3) as f32 * inverse_span;
				accumulation[row_start + right_index - 1] += delta * (1.0 - before_last_area - last_area);
			}
			accumulation[row_start + right_index] += delta * last_area;
		}
		position_x = next_position_x;
	}
}

//==============================================================================
// 그라데이션 색 멈춤 하나. (자리, 곧은 알파 색 0 ~ 1)
//==============================================================================
#[derive(Clone, Copy)]
struct GradientStop {
	offset: f32,
	color: [f32; 4],
}

//==============================================================================
// 색 줄을 멈춤 목록으로. (자리 차례로)
//==============================================================================
fn collect_gradient_stops(stop_iterator: impl Iterator<Item = rustybuzz::ttf_parser::colr::ColorStop>) -> Vec<GradientStop> {
	let mut stop_list: Vec<GradientStop> = stop_iterator.map(|color_stop| GradientStop {
		offset: color_stop.stop_offset,
		color: [color_stop.color.red as f32 / 255.0, color_stop.color.green as f32 / 255.0, color_stop.color.blue as f32 / 255.0, color_stop.color.alpha as f32 / 255.0],
	}).collect();
	stop_list.sort_by(|first_stop, second_stop| first_stop.offset.partial_cmp(&second_stop.offset).unwrap_or(std::cmp::Ordering::Equal));
	stop_list
}

//==============================================================================
// 색 줄에서 t 의 색. (끝 밖은 Pad, Repeat, Reflect. 곧은 알파로 섞어 프리멀티플라이드로 돌려준다)
//==============================================================================
fn evaluate_color_line(stop_list: &[GradientStop], extend: GradientExtend, t: f32) -> [f32; 4] {
	let first_stop = match stop_list.first() {
		Some(first_stop) => {
			first_stop
		}
		None => {
			return [0.0; 4];
		}
	};
	let last_stop = stop_list[stop_list.len() - 1];
	let span = last_stop.offset - first_stop.offset;
	let color = if span <= 1e-6 {
		if t < first_stop.offset {
			first_stop.color
		}
		else {
			last_stop.color
		}
	}
	else {
		let ratio = (t - first_stop.offset) / span;
		let extended_ratio = match extend {
			GradientExtend::Pad => {
				ratio.clamp(0.0, 1.0)
			}
			GradientExtend::Repeat => {
				ratio - ratio.floor()
			}
			GradientExtend::Reflect => {
				let folded_ratio = ratio.rem_euclid(2.0);
				if folded_ratio > 1.0 {
					2.0 - folded_ratio
				}
				else {
					folded_ratio
				}
			}
		};
		let position = first_stop.offset + extended_ratio * span;
		let mut color = last_stop.color;
		for stop_index in 0..stop_list.len() - 1 {
			let current_stop = stop_list[stop_index];
			let next_stop = stop_list[stop_index + 1];
			if position <= next_stop.offset {
				let stop_span = next_stop.offset - current_stop.offset;
				let interpolation = if stop_span <= 1e-6 {
					1.0
				}
				else {
					((position - current_stop.offset) / stop_span).clamp(0.0, 1.0)
				};
				for channel_index in 0..4 {
					color[channel_index] = current_stop.color[channel_index] + (next_stop.color[channel_index] - current_stop.color[channel_index]) * interpolation;
				}
				break;
			}
		}
		color
	};
	[color[0] * color[3], color[1] * color[3], color[2] * color[3], color[3]]
}

//==============================================================================
// 칠 하나를 픽셀마다 셈할 꼴로. (그라데이션은 멈춤을 모아 둔다)
//==============================================================================
enum PreparedPaint {
	Solid([f32; 4]),
	Linear { start_x: f32, start_y: f32, vector_x: f32, vector_y: f32, stop_list: Vec<GradientStop>, extend: GradientExtend },
	Radial { start_x: f32, start_y: f32, start_radius: f32, center_difference_x: f32, center_difference_y: f32, radius_difference: f32, stop_list: Vec<GradientStop>, extend: GradientExtend },
	Sweep { center_x: f32, center_y: f32, start_degrees: f32, end_degrees: f32, stop_list: Vec<GradientStop>, extend: GradientExtend },
}

impl PreparedPaint {
	//==============================================================================
	// 칠 만들기. (선형은 p0 에서 p1 을 p0p2 에 수직으로 돌린 p3 쪽, COLR 명세의 셈)
	//==============================================================================
	fn create(paint: &Paint<'static>) -> PreparedPaint {
		match paint {
			Paint::Solid(color) => {
				let alpha = color.alpha as f32 / 255.0;
				PreparedPaint::Solid([color.red as f32 / 255.0 * alpha, color.green as f32 / 255.0 * alpha, color.blue as f32 / 255.0 * alpha, alpha])
			}
			Paint::LinearGradient(linear_gradient) => {
				let first_vector_x = linear_gradient.x1 - linear_gradient.x0;
				let first_vector_y = linear_gradient.y1 - linear_gradient.y0;
				let second_vector_x = linear_gradient.x2 - linear_gradient.x0;
				let second_vector_y = linear_gradient.y2 - linear_gradient.y0;
				let perpendicular_x = second_vector_y;
				let perpendicular_y = -second_vector_x;
				let perpendicular_length_squared = perpendicular_x * perpendicular_x + perpendicular_y * perpendicular_y;
				let (vector_x, vector_y) = if perpendicular_length_squared <= 1e-12 {
					(first_vector_x, first_vector_y)
				}
				else {
					let projection = (first_vector_x * perpendicular_x + first_vector_y * perpendicular_y) / perpendicular_length_squared;
					(perpendicular_x * projection, perpendicular_y * projection)
				};
				PreparedPaint::Linear {
					start_x: linear_gradient.x0,
					start_y: linear_gradient.y0,
					vector_x,
					vector_y,
					stop_list: collect_gradient_stops(linear_gradient.stops(0, &[])),
					extend: linear_gradient.extend,
				}
			}
			Paint::RadialGradient(radial_gradient) => {
				PreparedPaint::Radial {
					start_x: radial_gradient.x0,
					start_y: radial_gradient.y0,
					start_radius: radial_gradient.r0,
					center_difference_x: radial_gradient.x1 - radial_gradient.x0,
					center_difference_y: radial_gradient.y1 - radial_gradient.y0,
					radius_difference: radial_gradient.r1 - radial_gradient.r0,
					stop_list: collect_gradient_stops(radial_gradient.stops(0, &[])),
					extend: radial_gradient.extend,
				}
			}
			Paint::SweepGradient(sweep_gradient) => {
				PreparedPaint::Sweep {
					center_x: sweep_gradient.center_x,
					center_y: sweep_gradient.center_y,
					start_degrees: sweep_gradient.start_angle * 180.0,
					end_degrees: sweep_gradient.end_angle * 180.0,
					stop_list: collect_gradient_stops(sweep_gradient.stops(0, &[])),
					extend: sweep_gradient.extend,
				}
			}
		}
	}

	//==============================================================================
	// 칠 공간의 점에서 색. (프리멀티플라이드, 방사는 두 원 사이에서 r(t) >= 0 인 큰 t)
	//==============================================================================
	fn evaluate(&self, position_x: f32, position_y: f32) -> [f32; 4] {
		match self {
			PreparedPaint::Solid(color) => {
				*color
			}
			PreparedPaint::Linear { start_x, start_y, vector_x, vector_y, stop_list, extend } => {
				let length_squared = vector_x * vector_x + vector_y * vector_y;
				if length_squared <= 1e-12 {
					return [0.0; 4];
				}
				let t = ((position_x - start_x) * vector_x + (position_y - start_y) * vector_y) / length_squared;
				evaluate_color_line(stop_list, *extend, t)
			}
			PreparedPaint::Radial { start_x, start_y, start_radius, center_difference_x, center_difference_y, radius_difference, stop_list, extend } => {
				let point_difference_x = position_x - start_x;
				let point_difference_y = position_y - start_y;
				let quadratic_a = center_difference_x * center_difference_x + center_difference_y * center_difference_y - radius_difference * radius_difference;
				let half_b = point_difference_x * center_difference_x + point_difference_y * center_difference_y + start_radius * radius_difference;
				let quadratic_c = point_difference_x * point_difference_x + point_difference_y * point_difference_y - start_radius * start_radius;
				let is_valid = |t: f32| start_radius + t * radius_difference >= 0.0;
				let t = if quadratic_a.abs() <= 1e-6 {
					if half_b.abs() <= 1e-12 {
						return [0.0; 4];
					}
					let t = quadratic_c / (2.0 * half_b);
					if !is_valid(t) {
						return [0.0; 4];
					}
					t
				}
				else {
					let discriminant = half_b * half_b - quadratic_a * quadratic_c;
					if discriminant < 0.0 {
						return [0.0; 4];
					}
					let root = discriminant.sqrt();
					let first_t = (half_b + root) / quadratic_a;
					let second_t = (half_b - root) / quadratic_a;
					let (larger_t, smaller_t) = if first_t > second_t {
						(first_t, second_t)
					}
					else {
						(second_t, first_t)
					};
					if is_valid(larger_t) {
						larger_t
					}
					else if is_valid(smaller_t) {
						smaller_t
					}
					else {
						return [0.0; 4];
					}
				};
				evaluate_color_line(stop_list, *extend, t)
			}
			PreparedPaint::Sweep { center_x, center_y, start_degrees, end_degrees, stop_list, extend } => {
				let span = end_degrees - start_degrees;
				if span.abs() <= 1e-6 {
					return [0.0; 4];
				}
				let mut angle_degrees = (position_y - center_y).atan2(position_x - center_x).to_degrees();
				if angle_degrees < 0.0 {
					angle_degrees += 360.0;
				}
				evaluate_color_line(stop_list, *extend, (angle_degrees - start_degrees) / span)
			}
		}
	}
}

//==============================================================================
// 색 그리개. (글꼴 단위 → 픽셀은 scale 배, y 를 뒤집어 판 왼쪽 위 기준)
//==============================================================================
struct ColorPainter<'face> {
	face: &'face Face<'static>,
	width: i32,
	height: i32,
	scale: f32,
	left_x: f32,
	top_y: f32,
	transform_stack: Vec<Transform>,
	transform: Transform,
	// 마지막으로 받은 윤곽. (픽셀 좌표 선분)
	outline_segment_list: Vec<[f32; 4]>,
	// 자르기 가면. (위의 것은 아래 것과 곱한 것)
	clip_stack: Vec<Vec<f32>>,
	// 합성할 층. (층 그림과 합성 방식)
	layer_stack: Vec<(Vec<[f32; 4]>, CompositeMode)>,
	canvas: Vec<[f32; 4]>,
}

impl ColorPainter<'_> {
	//==============================================================================
	// 글꼴 단위(지금 변환 앞) → 픽셀 변환.
	//==============================================================================
	fn read_pixel_transform(&self) -> Transform {
		let transform = &self.transform;
		Transform::new(transform.a * self.scale, -transform.b * self.scale, transform.c * self.scale, -transform.d * self.scale, transform.e * self.scale - self.left_x, self.top_y - transform.f * self.scale)
	}

	//==============================================================================
	// 지금 칠할 층. (층이 없으면 바탕 판)
	//==============================================================================
	fn read_target_buffer(&mut self) -> &mut Vec<[f32; 4]> {
		match self.layer_stack.last_mut() {
			Some((layer_buffer, _)) => {
				layer_buffer
			}
			None => {
				&mut self.canvas
			}
		}
	}

	//==============================================================================
	// 가면을 자르기 쌓기에 얹기. (위의 가면과 곱한다)
	//==============================================================================
	fn push_clip_mask(&mut self, mut mask: Vec<f32>) {
		if let Some(top_mask) = self.clip_stack.last() {
			for (value, top_value) in mask.iter_mut().zip(top_mask.iter()) {
				*value *= top_value;
			}
		}
		self.clip_stack.push(mask);
	}
}

impl<'face> Painter<'static> for ColorPainter<'face> {
	//==============================================================================
	// 윤곽 받기. (지금 변환으로 픽셀 좌표 선분에)
	//==============================================================================
	fn outline_glyph(&mut self, glyph_id: GlyphId) {
		let mut segment_collector = SegmentCollector::new(self.read_pixel_transform());
		self.face.outline_glyph(glyph_id, &mut segment_collector);
		self.outline_segment_list = segment_collector.segment_list;
	}

	//==============================================================================
	// 칠. (자르기가 있으면 자르기 영역, 없으면 마지막 윤곽을 칠 색으로 덮는다. 원본 위에 얹기)
	//==============================================================================
	fn paint(&mut self, paint: Paint<'static>) {
		let prepared_paint = PreparedPaint::create(&paint);
		let outline_mask;
		let mask: &[f32] = match self.clip_stack.last() {
			Some(top_mask) => {
				top_mask
			}
			None => {
				outline_mask = rasterize_segments(&self.outline_segment_list, self.width, self.height);
				&outline_mask
			}
		};
		// 픽셀 → 칠 공간. (지금 변환의 거꾸로)
		let transform = self.transform;
		let determinant = transform.a * transform.d - transform.b * transform.c;
		if determinant.abs() <= 1e-12 {
			return;
		}
		let width = self.width as usize;
		let scale = self.scale;
		let left_x = self.left_x;
		let top_y = self.top_y;
		let mut painted_list: Vec<(usize, [f32; 4])> = Vec::new();
		for (pixel_index, coverage) in mask.iter().enumerate() {
			if *coverage <= 0.0 {
				continue;
			}
			let font_x = ((pixel_index % width) as f32 + 0.5 + left_x) / scale - transform.e;
			let font_y = (top_y - ((pixel_index / width) as f32 + 0.5)) / scale - transform.f;
			let paint_x = (transform.d * font_x - transform.c * font_y) / determinant;
			let paint_y = (-transform.b * font_x + transform.a * font_y) / determinant;
			let color = prepared_paint.evaluate(paint_x, paint_y);
			painted_list.push((pixel_index, [color[0] * coverage, color[1] * coverage, color[2] * coverage, color[3] * coverage]));
		}
		let target_buffer = self.read_target_buffer();
		for (pixel_index, source_color) in painted_list {
			target_buffer[pixel_index] = composite_pixel(source_color, target_buffer[pixel_index], CompositeMode::SourceOver);
		}
	}

	//==============================================================================
	// 마지막 윤곽으로 자르기.
	//==============================================================================
	fn push_clip(&mut self) {
		let mask = rasterize_segments(&self.outline_segment_list, self.width, self.height);
		self.push_clip_mask(mask);
	}

	//==============================================================================
	// 자르기 상자로 자르기. (지금 변환을 건 사각형)
	//==============================================================================
	fn push_clip_box(&mut self, clip_box: ClipBox) {
		let mut segment_collector = SegmentCollector::new(self.read_pixel_transform());
		segment_collector.move_to(clip_box.x_min, clip_box.y_min);
		segment_collector.line_to(clip_box.x_max, clip_box.y_min);
		segment_collector.line_to(clip_box.x_max, clip_box.y_max);
		segment_collector.line_to(clip_box.x_min, clip_box.y_max);
		segment_collector.close();
		let mask = rasterize_segments(&segment_collector.segment_list, self.width, self.height);
		self.push_clip_mask(mask);
	}

	//==============================================================================
	// 자르기 빼기.
	//==============================================================================
	fn pop_clip(&mut self) {
		self.clip_stack.pop();
	}

	//==============================================================================
	// 새 층. (빼면 그 방식으로 아래에 합성한다)
	//==============================================================================
	fn push_layer(&mut self, mode: CompositeMode) {
		self.layer_stack.push((vec![[0.0; 4]; self.canvas.len()], mode));
	}

	//==============================================================================
	// 층 빼기. (층을 원본, 아래를 대상으로 합성)
	//==============================================================================
	fn pop_layer(&mut self) {
		let (layer_buffer, mode) = match self.layer_stack.pop() {
			Some(layer) => {
				layer
			}
			None => {
				return;
			}
		};
		let target_buffer = self.read_target_buffer();
		for (pixel_index, source_color) in layer_buffer.into_iter().enumerate() {
			target_buffer[pixel_index] = composite_pixel(source_color, target_buffer[pixel_index], mode);
		}
	}

	//==============================================================================
	// 변환 쌓기.
	//==============================================================================
	fn push_transform(&mut self, transform: Transform) {
		self.transform_stack.push(self.transform);
		self.transform = Transform::combine(self.transform, transform);
	}

	//==============================================================================
	// 변환 빼기.
	//==============================================================================
	fn pop_transform(&mut self) {
		if let Some(transform) = self.transform_stack.pop() {
			self.transform = transform;
		}
	}
}

//==============================================================================
// 프리멀티플라이드 색 합성. (원본 source 를 대상 destination 위에, COLR 의 합성 방식. W3C Compositing and Blending)
//==============================================================================
fn composite_pixel(source: [f32; 4], destination: [f32; 4], mode: CompositeMode) -> [f32; 4] {
	let source_alpha = source[3];
	let destination_alpha = destination[3];
	let porter_duff = |source_factor: f32, destination_factor: f32| -> [f32; 4] {
		[
			source[0] * source_factor + destination[0] * destination_factor,
			source[1] * source_factor + destination[1] * destination_factor,
			source[2] * source_factor + destination[2] * destination_factor,
			source[3] * source_factor + destination[3] * destination_factor,
		]
	};
	match mode {
		CompositeMode::Clear => {
			[0.0; 4]
		}
		CompositeMode::Source => {
			source
		}
		CompositeMode::Destination => {
			destination
		}
		CompositeMode::SourceOver => {
			porter_duff(1.0, 1.0 - source_alpha)
		}
		CompositeMode::DestinationOver => {
			porter_duff(1.0 - destination_alpha, 1.0)
		}
		CompositeMode::SourceIn => {
			porter_duff(destination_alpha, 0.0)
		}
		CompositeMode::DestinationIn => {
			porter_duff(0.0, source_alpha)
		}
		CompositeMode::SourceOut => {
			porter_duff(1.0 - destination_alpha, 0.0)
		}
		CompositeMode::DestinationOut => {
			porter_duff(0.0, 1.0 - source_alpha)
		}
		CompositeMode::SourceAtop => {
			porter_duff(destination_alpha, 1.0 - source_alpha)
		}
		CompositeMode::DestinationAtop => {
			porter_duff(1.0 - destination_alpha, source_alpha)
		}
		CompositeMode::Xor => {
			porter_duff(1.0 - destination_alpha, 1.0 - source_alpha)
		}
		CompositeMode::Plus => {
			[(source[0] + destination[0]).min(1.0), (source[1] + destination[1]).min(1.0), (source[2] + destination[2]).min(1.0), (source_alpha + destination_alpha).min(1.0)]
		}
		_ => {
			// 섞기: co = cs (1 - ab) + cb (1 - as) + as ab B(Cb, Cs), ao = as + ab - as ab
			let unpremultiply = |color: [f32; 4]| -> [f32; 3] {
				if color[3] <= 0.0 {
					[0.0; 3]
				}
				else {
					[color[0] / color[3], color[1] / color[3], color[2] / color[3]]
				}
			};
			let source_color = unpremultiply(source);
			let backdrop_color = unpremultiply(destination);
			let blended_color = blend_colors(backdrop_color, source_color, mode);
			let result_alpha = source_alpha + destination_alpha - source_alpha * destination_alpha;
			let mut result = [0.0f32; 4];
			for channel_index in 0..3 {
				result[channel_index] = source[channel_index] * (1.0 - destination_alpha) + destination[channel_index] * (1.0 - source_alpha) + source_alpha * destination_alpha * blended_color[channel_index];
			}
			result[3] = result_alpha;
			result
		}
	}
}

//==============================================================================
// 섞기 함수 B(Cb, Cs). (곧은 색, 나눌 수 있는 것은 채널마다, 색조 넷은 W3C 의 Lum, Sat 셈)
//==============================================================================
fn blend_colors(backdrop: [f32; 3], source: [f32; 3], mode: CompositeMode) -> [f32; 3] {
	let blend_channel = |backdrop_value: f32, source_value: f32| -> f32 {
		let hard_light = |backdrop_value: f32, source_value: f32| -> f32 {
			if source_value <= 0.5 {
				backdrop_value * 2.0 * source_value
			}
			else {
				let doubled = 2.0 * source_value - 1.0;
				backdrop_value + doubled - backdrop_value * doubled
			}
		};
		match mode {
			CompositeMode::Screen => {
				backdrop_value + source_value - backdrop_value * source_value
			}
			CompositeMode::Overlay => {
				hard_light(source_value, backdrop_value)
			}
			CompositeMode::Darken => {
				backdrop_value.min(source_value)
			}
			CompositeMode::Lighten => {
				backdrop_value.max(source_value)
			}
			CompositeMode::ColorDodge => {
				if backdrop_value <= 0.0 {
					0.0
				}
				else if source_value >= 1.0 {
					1.0
				}
				else {
					(backdrop_value / (1.0 - source_value)).min(1.0)
				}
			}
			CompositeMode::ColorBurn => {
				if backdrop_value >= 1.0 {
					1.0
				}
				else if source_value <= 0.0 {
					0.0
				}
				else {
					1.0 - ((1.0 - backdrop_value) / source_value).min(1.0)
				}
			}
			CompositeMode::HardLight => {
				hard_light(backdrop_value, source_value)
			}
			CompositeMode::SoftLight => {
				if source_value <= 0.5 {
					backdrop_value - (1.0 - 2.0 * source_value) * backdrop_value * (1.0 - backdrop_value)
				}
				else {
					let darkened = if backdrop_value <= 0.25 {
						((16.0 * backdrop_value - 12.0) * backdrop_value + 4.0) * backdrop_value
					}
					else {
						backdrop_value.sqrt()
					};
					backdrop_value + (2.0 * source_value - 1.0) * (darkened - backdrop_value)
				}
			}
			CompositeMode::Difference => {
				(backdrop_value - source_value).abs()
			}
			CompositeMode::Exclusion => {
				backdrop_value + source_value - 2.0 * backdrop_value * source_value
			}
			CompositeMode::Multiply => {
				backdrop_value * source_value
			}
			_ => {
				source_value
			}
		}
	};
	match mode {
		CompositeMode::Hue => {
			set_luminosity(set_saturation(source, read_saturation(backdrop)), read_luminosity(backdrop))
		}
		CompositeMode::Saturation => {
			set_luminosity(set_saturation(backdrop, read_saturation(source)), read_luminosity(backdrop))
		}
		CompositeMode::Color => {
			set_luminosity(source, read_luminosity(backdrop))
		}
		CompositeMode::Luminosity => {
			set_luminosity(backdrop, read_luminosity(source))
		}
		_ => {
			[blend_channel(backdrop[0], source[0]), blend_channel(backdrop[1], source[1]), blend_channel(backdrop[2], source[2])]
		}
	}
}

//==============================================================================
// 밝기. (W3C Lum)
//==============================================================================
fn read_luminosity(color: [f32; 3]) -> f32 {
	0.3 * color[0] + 0.59 * color[1] + 0.11 * color[2]
}

//==============================================================================
// 밝기 바꾸기. (W3C SetLum 과 ClipColor)
//==============================================================================
fn set_luminosity(color: [f32; 3], luminosity: f32) -> [f32; 3] {
	let difference = luminosity - read_luminosity(color);
	let mut result = [color[0] + difference, color[1] + difference, color[2] + difference];
	let current_luminosity = read_luminosity(result);
	let minimum = result[0].min(result[1]).min(result[2]);
	let maximum = result[0].max(result[1]).max(result[2]);
	if minimum < 0.0 && current_luminosity - minimum > 1e-6 {
		for value in result.iter_mut() {
			*value = current_luminosity + (*value - current_luminosity) * current_luminosity / (current_luminosity - minimum);
		}
	}
	if maximum > 1.0 && maximum - current_luminosity > 1e-6 {
		for value in result.iter_mut() {
			*value = current_luminosity + (*value - current_luminosity) * (1.0 - current_luminosity) / (maximum - current_luminosity);
		}
	}
	result
}

//==============================================================================
// 채도. (W3C Sat, 가장 큰 값 - 가장 작은 값)
//==============================================================================
fn read_saturation(color: [f32; 3]) -> f32 {
	color[0].max(color[1]).max(color[2]) - color[0].min(color[1]).min(color[2])
}

//==============================================================================
// 채도 바꾸기. (W3C SetSat)
//==============================================================================
fn set_saturation(color: [f32; 3], saturation: f32) -> [f32; 3] {
	let mut index_list = [0usize, 1, 2];
	index_list.sort_by(|first_index, second_index| color[*first_index].partial_cmp(&color[*second_index]).unwrap_or(std::cmp::Ordering::Equal));
	let (minimum_index, middle_index, maximum_index) = (index_list[0], index_list[1], index_list[2]);
	let mut result = [0.0f32; 3];
	let range = color[maximum_index] - color[minimum_index];
	if range > 1e-6 {
		result[middle_index] = (color[middle_index] - color[minimum_index]) * saturation / range;
		result[maximum_index] = saturation;
	}
	result
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 넓이 누적: 픽셀 경계에 맞춘 네모는 안이 1, 밖이 0, 반 걸친 열은 반.
	//==============================================================================
	#[test]
	fn rasterize_square_covers_inside() {
		let segment_list = [[2.0, 1.0, 2.0, 5.0], [2.0, 5.0, 6.5, 5.0], [6.5, 5.0, 6.5, 1.0], [6.5, 1.0, 2.0, 1.0]];
		let coverage = rasterize_segments(&segment_list, 10, 8);
		assert!((coverage[3 * 10 + 3] - 1.0).abs() < 1e-5);
		assert!(coverage[3 * 10 + 1].abs() < 1e-5);
		assert!((coverage[3 * 10 + 6] - 0.5).abs() < 1e-4);
		assert!(coverage[6 * 10 + 3].abs() < 1e-5);
		// 판 밖으로 나간 네모도 안쪽은 덮인다.
		let outside_segment_list = [[-5.0, -5.0, -5.0, 20.0], [-5.0, 20.0, 20.0, 20.0], [20.0, 20.0, 20.0, -5.0], [20.0, -5.0, -5.0, -5.0]];
		let outside_coverage = rasterize_segments(&outside_segment_list, 10, 8);
		assert!(outside_coverage.iter().all(|value| (value - 1.0).abs() < 1e-5));
	}

	//==============================================================================
	// 색 줄: Pad 는 끝 색, Repeat 는 되풀이, Reflect 는 되접기. 프리멀티플라이드로 돌려준다.
	//==============================================================================
	#[test]
	fn color_line_follows_extend() {
		let stop_list = [GradientStop { offset: 0.0, color: [1.0, 0.0, 0.0, 1.0] }, GradientStop { offset: 1.0, color: [0.0, 0.0, 1.0, 0.5] }];
		assert_eq!(evaluate_color_line(&stop_list, GradientExtend::Pad, -1.0), [1.0, 0.0, 0.0, 1.0]);
		assert_eq!(evaluate_color_line(&stop_list, GradientExtend::Pad, 2.0), [0.0, 0.0, 0.5, 0.5]);
		let repeated = evaluate_color_line(&stop_list, GradientExtend::Repeat, 1.25);
		let reflected = evaluate_color_line(&stop_list, GradientExtend::Reflect, 1.25);
		let quarter = evaluate_color_line(&stop_list, GradientExtend::Pad, 0.25);
		let three_quarter = evaluate_color_line(&stop_list, GradientExtend::Pad, 0.75);
		for channel_index in 0..4 {
			assert!((repeated[channel_index] - quarter[channel_index]).abs() < 1e-5);
			assert!((reflected[channel_index] - three_quarter[channel_index]).abs() < 1e-5);
		}
	}

	//==============================================================================
	// 합성: SourceOver 와 SourceIn, Multiply 가 W3C 식과 같다.
	//==============================================================================
	#[test]
	fn composite_matches_w3c() {
		let source = [0.5, 0.0, 0.0, 0.5];
		let destination = [0.0, 0.0, 1.0, 1.0];
		assert_eq!(composite_pixel(source, destination, CompositeMode::SourceOver), [0.5, 0.0, 0.5, 1.0]);
		assert_eq!(composite_pixel(source, destination, CompositeMode::SourceIn), [0.5, 0.0, 0.0, 0.5]);
		let multiplied = composite_pixel([1.0, 1.0, 1.0, 1.0], [0.2, 0.4, 0.6, 1.0], CompositeMode::Multiply);
		for channel_index in 0..3 {
			assert!((multiplied[channel_index] - [0.2, 0.4, 0.6][channel_index]).abs() < 1e-5);
		}
	}

	//==============================================================================
	// 다시 뽑기: 반으로 줄이면 두 픽셀이 하나로 평균, 반 픽셀 밀린 끝은 반만 덮인다.
	//==============================================================================
	#[test]
	fn resample_weights_average_and_cover_edges() {
		let weight_table = build_resample_weights(4, 0.0, 0.5, 2);
		assert_eq!(weight_table[0], vec![(0, 0.5), (1, 0.5)]);
		let shifted_table = build_resample_weights(2, 0.5, 1.0, 3);
		let edge_weight_sum: f32 = shifted_table[0].iter().map(|(_, weight)| weight).sum();
		assert!((edge_weight_sum - 0.5).abs() < 1e-5);
	}
}
