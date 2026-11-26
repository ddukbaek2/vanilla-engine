//==============================================================================
//! 타일셋. (원본 src/experimental/tilemap/tileset.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - 그림 한 장을 격자(타일 크기, 여백 margin, 간격 spacing)로 자르거나, 타일마다 그림을 따로 건다. (Tiled 의 그림 모음 타일셋)
//! - 타일마다 속성, 종류(type), 애니메이션 프레임, 충돌 모양, 그림 영역을 둘 수 있다. 타일 번호는 타일셋 안의 번호(0 부터)다.
//! - 모양과 정렬은 Tiled 를 따른다. (그리는 크기 tile / grid, 채우기 stretch / preserve-aspect-fit, 타일 오프셋은 아래가 +)
//! - 원본과 다른 점: 속성 값은 serde_json::Value 다. (원본은 JSON 값 그대로) 그림은 `Rc<Image>` 이고, 크기와 번호는 u32 다.
//==============================================================================
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::image::Image;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::rc::Rc;

//==============================================================================
/// 타일을 그리는 크기.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileRenderSize {
	/// 타일 그림 크기 그대로 그린다. (그림이 격자보다 크면 위와 오른쪽으로 넘친다)
	Tile,
	/// 지도의 격자 크기로 늘이거나 줄여 그린다.
	Grid,
}

//==============================================================================
/// 격자 크기로 그릴 때 채우는 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileFillMode {
	/// 격자에 꽉 차게 늘인다. (가로세로 비가 바뀔 수 있다)
	Stretch,
	/// 가로세로 비를 지켜 격자 안에 들어가게 줄이고 가운데 둔다.
	PreserveAspectFit,
}

//==============================================================================
/// 충돌 모양 종류. (Tiled 물체 모양과 같다)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileCollisionShapeKind {
	/// 사각형. (x, y 는 왼쪽 위, rotation 은 그 점을 축으로 돈다)
	Rectangle,
	/// 타원. (x, y, width, height 는 감싸는 사각형)
	Ellipse,
	/// 캡슐. (감싸는 사각형의 짧은 쪽이 지름인 둥근 막대)
	Capsule,
	/// 닫힌 다각형. (points 는 x, y 에서 잰 꼭짓점)
	Polygon,
	/// 열린 꺾은선. (points 는 x, y 에서 잰 점)
	Polyline,
	/// 점. (x, y 만 쓴다)
	Point,
}

//==============================================================================
/// 타일 애니메이션 프레임 하나.
///
/// # 예제
/// 물결 타일 프레임 둘
/// ```rust
/// use vanilla::experimental::tilemap::TileAnimationFrame;
///
/// let first_frame = TileAnimationFrame::new(10, 150);
/// let second_frame = TileAnimationFrame::new(11, 150);
/// assert_eq!(first_frame.duration + second_frame.duration, 300);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileAnimationFrame {
	/// 이 프레임에 보일 타일 번호. (타일셋 안의 번호)
	pub tile_id: u32,
	/// 이 프레임을 보이는 시간. (밀리초)
	pub duration: u32,
}

impl TileAnimationFrame {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `tile_id`: 이 프레임에 보일 타일 번호. (타일셋 안의 번호)
	/// - `duration`: 이 프레임을 보이는 시간. (밀리초)
	///
	/// # 돌려줌
	/// 새 프레임.
	//==============================================================================
	pub fn new(tile_id: u32, duration: u32) -> TileAnimationFrame {
		TileAnimationFrame {
			tile_id,
			duration,
		}
	}
}

//==============================================================================
/// 타일 충돌 모양 하나.
/// - 물리 엔진에 기대지 않는 자료다. 사각형, 타원, 캡슐은 x, y(왼쪽 위)와 너비, 높이, 그 점을 축으로 한 시계 방향 회전(도),
///   다각형과 꺾은선은 x, y 와 그 점에서 잰 꼭짓점 목록, 점은 x, y 만 쓴다. (Tiled 물체와 같은 꼴)
/// - create_transformed 로 타일 안 좌표를 지도 좌표로 옮긴다. (`Tilemap::read_cell_collision_shapes` 가 쓴다)
///
/// # 예제
/// 모양을 만들고 옮기기
/// ```rust
/// use vanilla::experimental::tilemap::{TileCollisionShape, TileCollisionShapeKind};
/// use vanilla::{TransformMatrix, Vector2};
///
/// let slope = TileCollisionShape::create_polygon(0.0, 16.0, &[Vector2::create(0.0, 0.0), Vector2::create(16.0, -16.0), Vector2::create(16.0, 0.0)]);
/// let mut move_right = TransformMatrix::identity();
/// move_right.translate(32.0, 0.0);
/// let moved_slope = slope.create_transformed(&move_right);
/// assert_eq!(moved_slope.kind, TileCollisionShapeKind::Polygon);
/// assert_eq!(moved_slope.x, 32.0);
/// assert_eq!(moved_slope.points.len(), 3);
/// ```
//==============================================================================
#[derive(Clone, Debug, PartialEq)]
pub struct TileCollisionShape {
	/// 모양 종류.
	pub kind: TileCollisionShapeKind,
	/// 기준점 X. (픽셀)
	pub x: f32,
	/// 기준점 Y. (픽셀)
	pub y: f32,
	/// 너비. (사각형, 타원, 캡슐, 픽셀)
	pub width: f32,
	/// 높이. (사각형, 타원, 캡슐, 픽셀)
	pub height: f32,
	/// 기준점을 축으로 한 시계 방향 회전. (도)
	pub rotation: f32,
	/// 기준점에서 잰 꼭짓점. (다각형, 꺾은선)
	pub points: Vec<Vector2>,
}

impl TileCollisionShape {
	//==============================================================================
	/// 생성. (값은 모두 0, 꼭짓점 없음)
	///
	/// # 인자
	/// - `kind`: 모양 종류.
	///
	/// # 돌려줌
	/// 새 모양.
	//==============================================================================
	pub fn new(kind: TileCollisionShapeKind) -> TileCollisionShape {
		TileCollisionShape {
			kind,
			x: 0.0,
			y: 0.0,
			width: 0.0,
			height: 0.0,
			rotation: 0.0,
			points: Vec::new(),
		}
	}

	//==============================================================================
	/// 사각형 만들기.
	///
	/// # 인자
	/// - `x`: 왼쪽. (픽셀)
	/// - `y`: 위. (픽셀)
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 새 사각형 모양.
	//==============================================================================
	pub fn create_rectangle(x: f32, y: f32, width: f32, height: f32) -> TileCollisionShape {
		let mut shape = TileCollisionShape::new(TileCollisionShapeKind::Rectangle);
		shape.x = x;
		shape.y = y;
		shape.width = width;
		shape.height = height;
		shape
	}

	//==============================================================================
	/// 타원 만들기. (감싸는 사각형으로)
	///
	/// # 인자
	/// - `x`: 감싸는 사각형의 왼쪽. (픽셀)
	/// - `y`: 감싸는 사각형의 위. (픽셀)
	/// - `width`: 너비. (픽셀)
	/// - `height`: 높이. (픽셀)
	///
	/// # 돌려줌
	/// 새 타원 모양.
	//==============================================================================
	pub fn create_ellipse(x: f32, y: f32, width: f32, height: f32) -> TileCollisionShape {
		let mut shape = TileCollisionShape::create_rectangle(x, y, width, height);
		shape.kind = TileCollisionShapeKind::Ellipse;
		shape
	}

	//==============================================================================
	/// 다각형 만들기. (닫힌 모양)
	///
	/// # 인자
	/// - `x`: 기준점 X. (픽셀)
	/// - `y`: 기준점 Y. (픽셀)
	/// - `points`: 기준점에서 잰 꼭짓점. (복사해 둔다)
	///
	/// # 돌려줌
	/// 새 다각형 모양.
	//==============================================================================
	pub fn create_polygon(x: f32, y: f32, points: &[Vector2]) -> TileCollisionShape {
		let mut shape = TileCollisionShape::new(TileCollisionShapeKind::Polygon);
		shape.x = x;
		shape.y = y;
		shape.points = points.to_vec();
		shape
	}

	//==============================================================================
	/// 꺾은선 만들기. (열린 모양)
	///
	/// # 인자
	/// - `x`: 기준점 X. (픽셀)
	/// - `y`: 기준점 Y. (픽셀)
	/// - `points`: 기준점에서 잰 점. (복사해 둔다)
	///
	/// # 돌려줌
	/// 새 꺾은선 모양.
	//==============================================================================
	pub fn create_polyline(x: f32, y: f32, points: &[Vector2]) -> TileCollisionShape {
		let mut shape = TileCollisionShape::create_polygon(x, y, points);
		shape.kind = TileCollisionShapeKind::Polyline;
		shape
	}

	//==============================================================================
	/// 점 만들기.
	///
	/// # 인자
	/// - `x`: X. (픽셀)
	/// - `y`: Y. (픽셀)
	///
	/// # 돌려줌
	/// 새 점 모양.
	//==============================================================================
	pub fn create_point(x: f32, y: f32) -> TileCollisionShape {
		let mut shape = TileCollisionShape::new(TileCollisionShapeKind::Point);
		shape.x = x;
		shape.y = y;
		shape
	}

	//==============================================================================
	/// 어파인 변환으로 옮긴 새 모양 만들기.
	/// - 다각형, 꺾은선, 점은 꼭짓점을 그대로 옮긴다. (다각형과 꺾은선의 회전은 꼭짓점에 녹이고 0 이 된다)
	/// - 사각형, 타원, 캡슐은 옮긴 두 축으로 다시 짓는다. 뒤집기(행렬식 < 0)는 반대쪽 모서리를 기준점으로 삼아 오른손 축으로 되돌린다.
	///   회전이 90 도의 배수가 되면 축에 맞춘 감싸는 사각형(회전 0)으로 고친다.
	///   두 축이 직각이 아니게 되면(기울임) 사각형은 다각형이 되고, 타원과 캡슐은 두 축 길이로 어림한다.
	///
	/// # 인자
	/// - `transform_matrix`: 옮길 변환. (x' = a*x + c*y + e, y' = b*x + d*y + f)
	///
	/// # 돌려줌
	/// 옮긴 새 모양.
	//==============================================================================
	pub fn create_transformed(&self, transform_matrix: &TransformMatrix) -> TileCollisionShape {
		let matrix_a = transform_matrix.a as f64;
		let matrix_b = transform_matrix.b as f64;
		let matrix_c = transform_matrix.c as f64;
		let matrix_d = transform_matrix.d as f64;
		let matrix_e = transform_matrix.e as f64;
		let matrix_f = transform_matrix.f as f64;
		let mut shape = TileCollisionShape::new(self.kind);
		let radian = (self.rotation as f64).to_radians();
		let cos_rotation = radian.cos();
		let sin_rotation = radian.sin();
		let source_x = self.x as f64;
		let source_y = self.y as f64;
		let origin_x = matrix_a * source_x + matrix_c * source_y + matrix_e;
		let origin_y = matrix_b * source_x + matrix_d * source_y + matrix_f;

		// 점, 다각형, 꺾은선.
		if self.kind == TileCollisionShapeKind::Point || self.kind == TileCollisionShapeKind::Polygon || self.kind == TileCollisionShapeKind::Polyline {
			shape.x = origin_x as f32;
			shape.y = origin_y as f32;
			for point in self.points.iter() {
				let point_x = point.x as f64;
				let point_y = point.y as f64;
				let rotated_x = point_x * cos_rotation - point_y * sin_rotation;
				let rotated_y = point_x * sin_rotation + point_y * cos_rotation;
				let transformed_x = matrix_a * rotated_x + matrix_c * rotated_y;
				let transformed_y = matrix_b * rotated_x + matrix_d * rotated_y;
				shape.points.push(Vector2::create(transformed_x as f32, transformed_y as f32));
			}
			return shape;
		}

		// 사각형, 타원, 캡슐. (두 축을 옮긴다)
		let width = self.width as f64;
		let height = self.height as f64;
		let mut axis_x_x = matrix_a * cos_rotation * width + matrix_c * sin_rotation * width;
		let mut axis_x_y = matrix_b * cos_rotation * width + matrix_d * sin_rotation * width;
		let axis_y_x = matrix_a * -sin_rotation * height + matrix_c * cos_rotation * height;
		let axis_y_y = matrix_b * -sin_rotation * height + matrix_d * cos_rotation * height;
		let mut corner_x = origin_x;
		let mut corner_y = origin_y;
		let determinant = matrix_a * matrix_d - matrix_b * matrix_c;
		if determinant < 0.0 {
			corner_x = origin_x + axis_x_x;
			corner_y = origin_y + axis_x_y;
			axis_x_x = -axis_x_x;
			axis_x_y = -axis_x_y;
		}
		let axis_x_length = (axis_x_x * axis_x_x + axis_x_y * axis_x_y).sqrt();
		let axis_y_length = (axis_y_x * axis_y_x + axis_y_y * axis_y_y).sqrt();
		let axis_dot = axis_x_x * axis_y_x + axis_x_y * axis_y_y;
		let is_sheared = axis_dot.abs() > 0.0001 * (axis_x_length * axis_y_length).max(1.0);
		if is_sheared && self.kind == TileCollisionShapeKind::Rectangle {
			shape.kind = TileCollisionShapeKind::Polygon;
			shape.x = corner_x as f32;
			shape.y = corner_y as f32;
			shape.points.push(Vector2::create(0.0, 0.0));
			shape.points.push(Vector2::create(axis_x_x as f32, axis_x_y as f32));
			shape.points.push(Vector2::create((axis_x_x + axis_y_x) as f32, (axis_x_y + axis_y_y) as f32));
			shape.points.push(Vector2::create(axis_y_x as f32, axis_y_y as f32));
			return shape;
		}
		let rotation = if axis_x_length > 0.0 {
			axis_x_y.atan2(axis_x_x).to_degrees()
		}
		else {
			0.0
		};
		let quarter_turn_count = (rotation / 90.0).round();
		if (rotation - quarter_turn_count * 90.0).abs() < 0.0001 {
			// 축에 맞춘 감싸는 사각형으로 고친다.
			let corner_x_list = [corner_x, corner_x + axis_x_x, corner_x + axis_y_x, corner_x + axis_x_x + axis_y_x];
			let corner_y_list = [corner_y, corner_y + axis_x_y, corner_y + axis_y_y, corner_y + axis_x_y + axis_y_y];
			let minimum_x = corner_x_list.iter().cloned().fold(f64::INFINITY, f64::min);
			let minimum_y = corner_y_list.iter().cloned().fold(f64::INFINITY, f64::min);
			let maximum_x = corner_x_list.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
			let maximum_y = corner_y_list.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
			shape.x = minimum_x as f32;
			shape.y = minimum_y as f32;
			shape.width = (maximum_x - minimum_x) as f32;
			shape.height = (maximum_y - minimum_y) as f32;
			shape.rotation = 0.0;
			return shape;
		}
		shape.x = corner_x as f32;
		shape.y = corner_y as f32;
		shape.width = axis_x_length as f32;
		shape.height = axis_y_length as f32;
		shape.rotation = rotation as f32;
		shape
	}
}

//==============================================================================
/// 타일 하나의 자료. (타일셋 안에서만 쓴다)
//==============================================================================
#[derive(Clone, Default)]
struct TileData {
	image: Option<Rc<Image>>,
	image_path: String,
	image_rect: Option<Rect>,
	tile_type: String,
	properties: Map<String, Value>,
	animation: Vec<TileAnimationFrame>,
	collision_shapes: Vec<TileCollisionShape>,
}

//==============================================================================
/// 타일셋.
///
/// # 예제
/// 격자 타일셋과 타일 속성, 애니메이션, 충돌 모양
/// ```rust
/// use vanilla::experimental::tilemap::{TileAnimationFrame, TileCollisionShape, Tileset};
///
/// let mut tileset = Tileset::new();
/// tileset.set_tile_size(16, 16);
/// tileset.set_margin(1);
/// tileset.set_spacing(2);
/// tileset.fit_grid_to_image_size(127, 37);
/// assert_eq!((tileset.get_columns(), tileset.get_tile_count()), (7, 14));
/// let image_rect = tileset.read_tile_image_rect(8);
/// assert_eq!((image_rect.position.x, image_rect.position.y), (19.0, 19.0));
///
/// tileset.set_tile_property(3, "isWater", serde_json::json!(true));
/// tileset.set_tile_animation(3, &[TileAnimationFrame::new(3, 200), TileAnimationFrame::new(4, 200)]);
/// assert_eq!(tileset.get_tile_property(3, "isWater"), Some(&serde_json::json!(true)));
/// assert_eq!(tileset.read_animated_tile_id(3, 250.0), 4);
///
/// tileset.set_tile_collision_shapes(5, &[TileCollisionShape::create_rectangle(0.0, 8.0, 16.0, 8.0)]);
/// assert_eq!(tileset.get_tile_collision_shapes(5).len(), 1);
/// ```
//==============================================================================
#[derive(Clone)]
pub struct Tileset {
	name: String,
	image: Option<Rc<Image>>,
	image_path: String,
	tile_width: u32,
	tile_height: u32,
	margin: u32,
	spacing: u32,
	columns: u32,
	tile_count: u32,
	tile_offset: Vector2,
	tile_render_size: TileRenderSize,
	fill_mode: TileFillMode,
	properties: Map<String, Value>,
	tile_data_table: HashMap<u32, TileData>,
}

impl Tileset {
	//==============================================================================
	/// 생성. (타일 32 x 32, 격자 없음)
	///
	/// # 돌려줌
	/// 빈 타일셋.
	//==============================================================================
	pub fn new() -> Tileset {
		Tileset {
			name: String::new(),
			image: None,
			image_path: String::new(),
			tile_width: 32,
			tile_height: 32,
			margin: 0,
			spacing: 0,
			columns: 0,
			tile_count: 0,
			tile_offset: Vector2::zero(),
			tile_render_size: TileRenderSize::Tile,
			fill_mode: TileFillMode::Stretch,
			properties: Map::new(),
			tile_data_table: HashMap::new(),
		}
	}

	//==============================================================================
	/// 그림 한 장의 격자 타일셋 만들기. (그림 크기로 열 수와 타일 수를 셈한다)
	///
	/// # 인자
	/// - `image`: 자를 그림.
	/// - `tile_width`: 타일 너비. (픽셀)
	/// - `tile_height`: 타일 높이. (픽셀)
	/// - `margin`: 그림 가장자리와 첫 타일 사이. (픽셀)
	/// - `spacing`: 이웃 타일 사이. (픽셀)
	///
	/// # 돌려줌
	/// 새 타일셋.
	///
	/// # 예제
	/// 그림 한 장으로 격자 타일셋 만들기
	/// ```rust
	/// use vanilla::experimental::tilemap::Tileset;
	/// use vanilla::Image;
	///
	/// let image = Image::create_from_pixels(144, 54, vec![0; 144 * 54 * 4]);
	/// let tileset = Tileset::create_grid(image, 16, 16, 1, 2);
	/// assert_eq!((tileset.get_columns(), tileset.get_tile_count()), (8, 24));
	/// ```
	//==============================================================================
	pub fn create_grid(image: Rc<Image>, tile_width: u32, tile_height: u32, margin: u32, spacing: u32) -> Tileset {
		let mut tileset = Tileset::new();
		let image_width = image.get_width();
		let image_height = image.get_height();
		tileset.set_image(Some(image));
		tileset.set_tile_size(tile_width, tile_height);
		tileset.set_margin(margin);
		tileset.set_spacing(spacing);
		tileset.fit_grid_to_image_size(image_width, image_height);
		tileset
	}

	//==============================================================================
	/// 그림 크기로 열 수와 타일 수 셈하기. (타일 크기, 여백, 간격을 먼저 정한다)
	///
	/// # 인자
	/// - `image_width`: 그림 너비. (픽셀)
	/// - `image_height`: 그림 높이. (픽셀)
	//==============================================================================
	pub fn fit_grid_to_image_size(&mut self, image_width: u32, image_height: u32) {
		if self.tile_width == 0 || self.tile_height == 0 {
			self.columns = 0;
			self.tile_count = 0;
			return;
		}
		let usable_width = image_width as i64 - self.margin as i64 * 2 + self.spacing as i64;
		let usable_height = image_height as i64 - self.margin as i64 * 2 + self.spacing as i64;
		let columns = (usable_width.max(0) / (self.tile_width + self.spacing) as i64) as u32;
		let rows = (usable_height.max(0) / (self.tile_height + self.spacing) as i64) as u32;
		self.columns = columns;
		self.tile_count = columns * rows;
	}

	//==============================================================================
	/// 이름 설정.
	///
	/// # 인자
	/// - `name`: 이름.
	//==============================================================================
	pub fn set_name(&mut self, name: &str) {
		self.name = String::from(name);
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 이름. (처음은 빈 글)
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 격자로 자를 그림 설정.
	///
	/// # 인자
	/// - `image`: 그림. (None 이면 비운다)
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	/// 격자로 자를 그림 반환.
	///
	/// # 돌려줌
	/// 그림. (없으면 None)
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	/// 그림 경로 설정. (읽은 파일에서 온 경로, 그림은 set_image 로 따로 건다)
	///
	/// # 인자
	/// - `image_path`: 그림 경로.
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	/// 그림 경로 반환.
	///
	/// # 돌려줌
	/// 그림 경로. (없으면 빈 글)
	//==============================================================================
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}

	//==============================================================================
	/// 타일 크기 설정.
	///
	/// # 인자
	/// - `tile_width`: 타일 너비. (픽셀)
	/// - `tile_height`: 타일 높이. (픽셀)
	//==============================================================================
	pub fn set_tile_size(&mut self, tile_width: u32, tile_height: u32) {
		self.tile_width = tile_width;
		self.tile_height = tile_height;
	}

	//==============================================================================
	/// 타일 너비 반환.
	///
	/// # 돌려줌
	/// 타일 너비. (픽셀, 처음은 32)
	//==============================================================================
	pub fn get_tile_width(&self) -> u32 {
		self.tile_width
	}

	//==============================================================================
	/// 타일 높이 반환.
	///
	/// # 돌려줌
	/// 타일 높이. (픽셀, 처음은 32)
	//==============================================================================
	pub fn get_tile_height(&self) -> u32 {
		self.tile_height
	}

	//==============================================================================
	/// 여백 설정. (그림 가장자리와 첫 타일 사이)
	///
	/// # 인자
	/// - `margin`: 여백. (픽셀)
	//==============================================================================
	pub fn set_margin(&mut self, margin: u32) {
		self.margin = margin;
	}

	//==============================================================================
	/// 여백 반환.
	///
	/// # 돌려줌
	/// 여백. (픽셀, 처음은 0)
	//==============================================================================
	pub fn get_margin(&self) -> u32 {
		self.margin
	}

	//==============================================================================
	/// 간격 설정. (이웃 타일 사이)
	///
	/// # 인자
	/// - `spacing`: 간격. (픽셀)
	//==============================================================================
	pub fn set_spacing(&mut self, spacing: u32) {
		self.spacing = spacing;
	}

	//==============================================================================
	/// 간격 반환.
	///
	/// # 돌려줌
	/// 간격. (픽셀, 처음은 0)
	//==============================================================================
	pub fn get_spacing(&self) -> u32 {
		self.spacing
	}

	//==============================================================================
	/// 격자의 열 수 설정.
	///
	/// # 인자
	/// - `columns`: 열 수. (0 이면 격자 없이 타일마다 그림을 건다)
	//==============================================================================
	pub fn set_columns(&mut self, columns: u32) {
		self.columns = columns;
	}

	//==============================================================================
	/// 격자의 열 수 반환.
	///
	/// # 돌려줌
	/// 열 수. (처음은 0)
	//==============================================================================
	pub fn get_columns(&self) -> u32 {
		self.columns
	}

	//==============================================================================
	/// 타일 수 설정.
	///
	/// # 인자
	/// - `tile_count`: 타일 수.
	//==============================================================================
	pub fn set_tile_count(&mut self, tile_count: u32) {
		self.tile_count = tile_count;
	}

	//==============================================================================
	/// 타일 수 반환.
	///
	/// # 돌려줌
	/// 타일 수. (처음은 0)
	//==============================================================================
	pub fn get_tile_count(&self) -> u32 {
		self.tile_count
	}

	//==============================================================================
	/// 타일 오프셋 설정. (그릴 때 더하는 자리)
	///
	/// # 인자
	/// - `tile_offset`: 오프셋. (픽셀, 아래가 +)
	//==============================================================================
	pub fn set_tile_offset(&mut self, tile_offset: Vector2) {
		self.tile_offset = tile_offset;
	}

	//==============================================================================
	/// 타일 오프셋 반환.
	///
	/// # 돌려줌
	/// 오프셋. (픽셀, 처음은 0, 0)
	//==============================================================================
	pub fn get_tile_offset(&self) -> Vector2 {
		self.tile_offset
	}

	//==============================================================================
	/// 그리는 크기 설정.
	///
	/// # 인자
	/// - `tile_render_size`: 그리는 크기.
	//==============================================================================
	pub fn set_tile_render_size(&mut self, tile_render_size: TileRenderSize) {
		self.tile_render_size = tile_render_size;
	}

	//==============================================================================
	/// 그리는 크기 반환.
	///
	/// # 돌려줌
	/// 그리는 크기. (처음은 Tile)
	//==============================================================================
	pub fn get_tile_render_size(&self) -> TileRenderSize {
		self.tile_render_size
	}

	//==============================================================================
	/// 채우기 설정. (격자 크기로 그릴 때)
	///
	/// # 인자
	/// - `fill_mode`: 채우기.
	//==============================================================================
	pub fn set_fill_mode(&mut self, fill_mode: TileFillMode) {
		self.fill_mode = fill_mode;
	}

	//==============================================================================
	/// 채우기 반환.
	///
	/// # 돌려줌
	/// 채우기. (처음은 Stretch)
	//==============================================================================
	pub fn get_fill_mode(&self) -> TileFillMode {
		self.fill_mode
	}

	//==============================================================================
	/// 타일셋 속성 설정.
	///
	/// # 인자
	/// - `name`: 속성 이름.
	/// - `value`: 값. (Tiled 속성 값 그대로, 색은 "#AARRGGBB" 글)
	//==============================================================================
	pub fn set_property(&mut self, name: &str, value: Value) {
		self.properties.insert(String::from(name), value);
	}

	//==============================================================================
	/// 타일셋 속성 반환.
	///
	/// # 인자
	/// - `name`: 속성 이름.
	///
	/// # 돌려줌
	/// 값. (없으면 None)
	//==============================================================================
	pub fn get_property(&self, name: &str) -> Option<&Value> {
		self.properties.get(name)
	}

	//==============================================================================
	/// 타일셋 속성 모두 반환.
	///
	/// # 돌려줌
	/// 이름 → 값. (넣은 차례)
	//==============================================================================
	pub fn get_properties(&self) -> &Map<String, Value> {
		&self.properties
	}

	//==============================================================================
	/// 타일 자료 반환. (없으면 만든다)
	//==============================================================================
	fn ensure_tile_data(&mut self, tile_id: u32) -> &mut TileData {
		self.tile_data_table.entry(tile_id).or_default()
	}

	//==============================================================================
	/// 자료(그림, 속성, 애니메이션, 충돌 모양 등)를 가진 타일 번호 목록 반환.
	///
	/// # 돌려줌
	/// 타일 번호. (작은 차례)
	//==============================================================================
	pub fn get_defined_tile_ids(&self) -> Vec<u32> {
		let mut tile_id_list: Vec<u32> = self.tile_data_table.keys().cloned().collect();
		tile_id_list.sort_unstable();
		tile_id_list
	}

	//==============================================================================
	/// 타일 하나의 그림 설정. (그림 모음 타일셋, 그 타일만 이 그림으로 그린다)
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `image`: 그림. (None 이면 타일셋 그림을 쓴다)
	//==============================================================================
	pub fn set_tile_image(&mut self, tile_id: u32, image: Option<Rc<Image>>) {
		self.ensure_tile_data(tile_id).image = image;
	}

	//==============================================================================
	/// 타일 하나의 그림 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 그 타일의 그림. (따로 건 것이 없으면 타일셋 그림, 그것도 없으면 None)
	//==============================================================================
	pub fn get_tile_image(&self, tile_id: u32) -> Option<&Rc<Image>> {
		if let Some(tile_data) = self.tile_data_table.get(&tile_id) {
			if let Some(image) = &tile_data.image {
				return Some(image);
			}
		}
		self.image.as_ref()
	}

	//==============================================================================
	/// 타일 하나에 따로 건 그림이 있는지.
	//==============================================================================
	pub(crate) fn has_own_tile_image(&self, tile_id: u32) -> bool {
		match self.tile_data_table.get(&tile_id) {
			Some(tile_data) => {
				tile_data.image.is_some()
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 타일 하나의 그림 경로 설정. (읽은 파일에서 온 경로)
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `image_path`: 그림 경로.
	//==============================================================================
	pub fn set_tile_image_path(&mut self, tile_id: u32, image_path: &str) {
		self.ensure_tile_data(tile_id).image_path = String::from(image_path);
	}

	//==============================================================================
	/// 타일 하나의 그림 경로 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 그림 경로. (없으면 빈 글)
	//==============================================================================
	pub fn get_tile_image_path(&self, tile_id: u32) -> &str {
		match self.tile_data_table.get(&tile_id) {
			Some(tile_data) => {
				&tile_data.image_path
			}
			None => {
				""
			}
		}
	}

	//==============================================================================
	/// 타일 하나의 그림 영역 설정. (격자 대신 그림 안의 이 영역을 쓴다)
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `image_rect`: 그림 안의 영역. (픽셀, None 이면 격자나 그림 전체)
	//==============================================================================
	pub fn set_tile_image_rect(&mut self, tile_id: u32, image_rect: Option<Rect>) {
		self.ensure_tile_data(tile_id).image_rect = image_rect;
	}

	//==============================================================================
	/// 타일 하나에 따로 건 그림 영역 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 따로 건 영역. (없으면 None)
	//==============================================================================
	pub fn get_tile_image_rect(&self, tile_id: u32) -> Option<Rect> {
		self.tile_data_table.get(&tile_id).and_then(|tile_data| tile_data.image_rect)
	}

	//==============================================================================
	/// 타일 하나를 그릴 그림 영역 셈하기.
	/// - 따로 건 영역, 격자(열 수가 있으면), 그 타일 그림 전체의 차례로 고른다.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 그림 안의 영역. (픽셀, 알 수 없으면 0 크기)
	//==============================================================================
	pub fn read_tile_image_rect(&self, tile_id: u32) -> Rect {
		if let Some(image_rect) = self.get_tile_image_rect(tile_id) {
			return image_rect;
		}
		if !self.has_own_tile_image(tile_id) && self.columns > 0 {
			let column = tile_id % self.columns;
			let row = tile_id / self.columns;
			let left = self.margin + column * (self.tile_width + self.spacing);
			let top = self.margin + row * (self.tile_height + self.spacing);
			return Rect::create(left as f32, top as f32, self.tile_width as f32, self.tile_height as f32);
		}
		match self.get_tile_image(tile_id) {
			Some(image) => {
				Rect::create(0.0, 0.0, image.get_width() as f32, image.get_height() as f32)
			}
			None => {
				Rect::zero()
			}
		}
	}

	//==============================================================================
	/// 타일 종류 설정. (Tiled 의 class, 언리얼의 user data name 같은 꼬리표)
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `tile_type`: 종류 이름.
	//==============================================================================
	pub fn set_tile_type(&mut self, tile_id: u32, tile_type: &str) {
		self.ensure_tile_data(tile_id).tile_type = String::from(tile_type);
	}

	//==============================================================================
	/// 타일 종류 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 종류 이름. (없으면 빈 글)
	//==============================================================================
	pub fn get_tile_type(&self, tile_id: u32) -> &str {
		match self.tile_data_table.get(&tile_id) {
			Some(tile_data) => {
				&tile_data.tile_type
			}
			None => {
				""
			}
		}
	}

	//==============================================================================
	/// 타일 속성 설정.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `name`: 속성 이름.
	/// - `value`: 값.
	//==============================================================================
	pub fn set_tile_property(&mut self, tile_id: u32, name: &str, value: Value) {
		self.ensure_tile_data(tile_id).properties.insert(String::from(name), value);
	}

	//==============================================================================
	/// 타일 속성 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `name`: 속성 이름.
	///
	/// # 돌려줌
	/// 값. (없으면 None)
	//==============================================================================
	pub fn get_tile_property(&self, tile_id: u32, name: &str) -> Option<&Value> {
		self.tile_data_table.get(&tile_id).and_then(|tile_data| tile_data.properties.get(name))
	}

	//==============================================================================
	/// 타일 속성 모두 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 이름 → 값. (없으면 None)
	//==============================================================================
	pub fn get_tile_properties(&self, tile_id: u32) -> Option<&Map<String, Value>> {
		self.tile_data_table.get(&tile_id).map(|tile_data| &tile_data.properties)
	}

	//==============================================================================
	/// 타일 애니메이션 설정.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호. (지도에 놓인 번호, 프레임마다 다른 타일로 바뀌어 보인다)
	/// - `frame_list`: 프레임 목록. (빈 목록이면 애니메이션을 끈다, 복사해 둔다)
	//==============================================================================
	pub fn set_tile_animation(&mut self, tile_id: u32, frame_list: &[TileAnimationFrame]) {
		self.ensure_tile_data(tile_id).animation = frame_list.to_vec();
	}

	//==============================================================================
	/// 타일 애니메이션 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 프레임 목록. (없으면 빈 목록)
	//==============================================================================
	pub fn get_tile_animation(&self, tile_id: u32) -> &[TileAnimationFrame] {
		match self.tile_data_table.get(&tile_id) {
			Some(tile_data) => {
				&tile_data.animation
			}
			None => {
				&[]
			}
		}
	}

	//==============================================================================
	/// 시각에 보일 애니메이션 타일 번호 셈하기.
	/// - 모든 칸이 같은 시계로 돈다. (Tiled 와 같다) 프레임 시간 합으로 나눈 나머지 시각이 든 프레임을 고른다.
	/// - 애니메이션이 없으면 그 번호, 시간 합이 0 이면 첫 프레임이다.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `time_milliseconds`: 시각. (밀리초, 음수도 된다)
	///
	/// # 돌려줌
	/// 보일 타일 번호.
	//==============================================================================
	pub fn read_animated_tile_id(&self, tile_id: u32, time_milliseconds: f64) -> u32 {
		let frame_list = self.get_tile_animation(tile_id);
		if frame_list.is_empty() {
			return tile_id;
		}
		let total_duration: f64 = frame_list.iter().map(|frame| frame.duration as f64).sum();
		if total_duration <= 0.0 {
			return frame_list[0].tile_id;
		}
		let mut remaining_time = time_milliseconds % total_duration;
		if remaining_time < 0.0 {
			remaining_time += total_duration;
		}
		for frame in frame_list.iter() {
			let duration = frame.duration as f64;
			if remaining_time < duration {
				return frame.tile_id;
			}
			remaining_time -= duration;
		}
		frame_list[frame_list.len() - 1].tile_id
	}

	//==============================================================================
	/// 타일 충돌 모양 설정.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	/// - `shape_list`: 모양 목록. (타일 그림의 왼쪽 위가 원점인 픽셀 좌표, 복사해 둔다)
	//==============================================================================
	pub fn set_tile_collision_shapes(&mut self, tile_id: u32, shape_list: &[TileCollisionShape]) {
		self.ensure_tile_data(tile_id).collision_shapes = shape_list.to_vec();
	}

	//==============================================================================
	/// 타일 충돌 모양 반환.
	///
	/// # 인자
	/// - `tile_id`: 타일 번호.
	///
	/// # 돌려줌
	/// 모양 목록. (없으면 빈 목록)
	//==============================================================================
	pub fn get_tile_collision_shapes(&self, tile_id: u32) -> &[TileCollisionShape] {
		match self.tile_data_table.get(&tile_id) {
			Some(tile_data) => {
				&tile_data.collision_shapes
			}
			None => {
				&[]
			}
		}
	}

	//==============================================================================
	/// 그릴 때 격자 밖으로 넘칠 수 있는 가장 큰 크기 셈하기. (화면 밖 거르기의 여유)
	/// - 타일 크기, 따로 건 그림 영역과 그림 크기, 오프셋의 크기 중 가장 큰 것.
	///
	/// # 돌려줌
	/// 가장 큰 크기. (픽셀)
	//==============================================================================
	pub fn read_maximum_tile_extent(&self) -> f32 {
		let mut maximum_extent = self.tile_width.max(self.tile_height) as f32;
		for tile_data in self.tile_data_table.values() {
			if let Some(image_rect) = tile_data.image_rect {
				maximum_extent = maximum_extent.max(image_rect.size.x).max(image_rect.size.y);
			}
			else if let Some(image) = &tile_data.image {
				maximum_extent = maximum_extent.max(image.get_width() as f32).max(image.get_height() as f32);
			}
		}
		maximum_extent + self.tile_offset.x.abs() + self.tile_offset.y.abs()
	}
}

impl Default for Tileset {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Tileset {
		Tileset::new()
	}
}
