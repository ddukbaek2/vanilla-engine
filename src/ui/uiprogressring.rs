//==============================================================================
//! 진행 고리. (vanilla.js ui/uiprogressring.js 의 UIProgressRing)
//! - 노드의 꼴(둥근 네모, 둥근 정도가 크기의 반이면 동그라미)을 따라 도는 가는 선이다.
//! - 위 가운데에서 시계 방향으로 채운 몫만큼 채운 색, 나머지는 바탕 색으로 그린다.
//! - 붙인 노드의 내용 크기 전체가 고리의 바깥 꼴이고, 선의 가운데가 그 안쪽으로 굵기의 반만큼 들어온다.
//! - 둘레의 점은 크기, 둥근 정도, 굵기가 바뀔 때만 다시 셈한다.
//==============================================================================
use std::f32::consts::PI;

use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

/// 둥근 귀퉁이 하나를 나누는 점의 수. (고리가 매끈해 보일 만큼)
const CORNER_STEPS: usize = 10;

//==============================================================================
/// 진행 고리.
/// - 붙인 노드의 내용 크기를 바깥 꼴로, 위 가운데에서 시계 방향으로 채운 몫을 그린다.
///
/// # 예제
/// 48 x 48 동그라미 고리
/// ```rust
/// use vanilla::{NodeTree, UIProgressRing, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let ring_id = node_tree.create_child(root_id, "LoadingRing");
/// node_tree[ring_id].set_content_size(Vector2::create(48.0, 48.0));
/// let ring = node_tree.add_component(ring_id, UIProgressRing::new()).expect("ring");
/// ring.set_round_size(24.0);
/// ring.set_line_width(4.0);
/// ring.set_ratio(0.25);
/// assert_eq!(ring.get_ratio(), 0.25);
/// ring.set_ratio(1.5);
/// assert_eq!(ring.get_ratio(), 1.0);
/// ```
//==============================================================================
pub struct UIProgressRing {
	component_base: ComponentBase,
	ratio: f32,
	round_size: f32,
	line_width: f32,
	fill_color: Color,
	track_color: Color,
	perimeter_points: Vec<Vector2>,
	perimeter_key: [f32; 4],
}

#[crate::reflect]
impl UIProgressRing {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 붙기 전의 고리. (몫 0, 둥근 정도 0, 굵기 2, 채운 색 흰색)
	//==============================================================================
	pub fn new() -> UIProgressRing {
		UIProgressRing {
			component_base: ComponentBase::new(),
			ratio: 0.0,
			round_size: 0.0,
			line_width: 2.0,
			fill_color: Color::white(),
			track_color: Color::create(0.2, 0.2, 0.25, 1.0),
			perimeter_points: Vec::new(),
			perimeter_key: [-1.0; 4],
		}
	}

	//==============================================================================
	/// 출력. (바탕 고리 한 바퀴, 그 위에 채운 몫)
	///
	/// # 인자
	/// - `transform`: 노드 변환. (내용 크기가 고리의 바깥 꼴, 크기에서 굵기를 뺀 가로나 세로가 1 이하면 그리지 않는다)
	/// - `graphic`: 그릴 그래픽.
	//==============================================================================
	pub fn draw_ring(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let line_width = self.line_width;
		let inset = line_width * 0.5;
		let width = content_size.x - line_width;
		let height = content_size.y - line_width;
		if width <= 1.0 || height <= 1.0 {
			return;
		}
		let round_size = (self.round_size - inset).max(0.0);
		let perimeter_key = [width, height, round_size, inset];
		if perimeter_key != self.perimeter_key {
			self.perimeter_points = UIProgressRing::read_perimeter_points(inset, inset, width, height, round_size);
			self.perimeter_key = perimeter_key;
		}
		graphic.set_stroke_color(self.track_color);
		graphic.draw_line_loop(&self.perimeter_points, line_width);
		if self.ratio <= 0.0 {
			return;
		}
		graphic.set_stroke_color(self.fill_color);
		if self.ratio >= 1.0 {
			graphic.draw_line_loop(&self.perimeter_points, line_width);
			return;
		}
		let partial_points = UIProgressRing::read_partial_points(&self.perimeter_points, self.ratio);
		graphic.draw_line(&partial_points, line_width);
	}

	//==============================================================================
	/// 둥근 네모 둘레의 점. (위 가운데에서 시작해 시계 방향, 닫는 점은 넣지 않는다)
	///
	/// # 인자
	/// - `left`: 네모의 왼쪽.
	/// - `top`: 네모의 위.
	/// - `width`: 네모의 가로.
	/// - `height`: 네모의 세로.
	/// - `round_size`: 귀퉁이 반지름. (가로와 세로의 반을 넘으면 그 값으로 자른다)
	///
	/// # 돌려줌
	/// 둘레의 점 45 개. (위 가운데 1 개, 귀퉁이마다 11 개)
	///
	/// # 예제
	/// 첫 점은 위 가운데
	/// ```rust
	/// use vanilla::{UIProgressRing, Vector2};
	///
	/// let points = UIProgressRing::read_perimeter_points(0.0, 0.0, 100.0, 60.0, 0.0);
	/// assert_eq!(points.len(), 45);
	/// assert_eq!(points[0], Vector2::create(50.0, 0.0));
	/// ```
	//==============================================================================
	pub fn read_perimeter_points(left: f32, top: f32, width: f32, height: f32, round_size: f32) -> Vec<Vector2> {
		let radius = round_size.min(width * 0.5).min(height * 0.5);
		let right = left + width;
		let bottom = top + height;
		let mut points = vec![Vector2::create(left + width * 0.5, top)];

		// 귀퉁이마다 가운데와 시작 각. (화면 좌표는 아래가 + 라 시계 방향이 각이 느는 쪽이다)
		let corners = [
			(right - radius, top + radius, -PI * 0.5),
			(right - radius, bottom - radius, 0.0),
			(left + radius, bottom - radius, PI * 0.5),
			(left + radius, top + radius, PI),
		];
		for (center_x, center_y, start_angle) in corners {
			for step_index in 0..=CORNER_STEPS {
				let angle = start_angle + PI * 0.5 * step_index as f32 / CORNER_STEPS as f32;
				points.push(Vector2::create(center_x + angle.cos() * radius, center_y + angle.sin() * radius));
			}
		}
		points
	}

	//==============================================================================
	/// 둘레의 앞 몫만큼의 점. (길이로 셈하고, 끝점은 마지막 변 위에서 잘라 넣는다)
	///
	/// # 인자
	/// - `points`: 둘레의 점. (닫는 점 없이, read_perimeter_points 의 결과)
	/// - `ratio`: 앞에서부터 그릴 몫. (닫은 둘레 길이의 비율, 0 ~ 1)
	///
	/// # 돌려줌
	/// 첫 점부터 그 몫까지 이은 점. (points 가 비면 빈 목록, 1 이상이면 첫 점으로 돌아와 닫는다)
	///
	/// # 예제
	/// 반 바퀴면 아래 가운데에서 끝난다
	/// ```rust
	/// use vanilla::UIProgressRing;
	///
	/// let points = UIProgressRing::read_perimeter_points(0.0, 0.0, 100.0, 100.0, 0.0);
	/// let half_points = UIProgressRing::read_partial_points(&points, 0.5);
	/// let last_point = half_points[half_points.len() - 1];
	/// assert!((last_point.x - 50.0).abs() < 0.01);
	/// assert!((last_point.y - 100.0).abs() < 0.01);
	/// ```
	//==============================================================================
	pub fn read_partial_points(points: &[Vector2], ratio: f32) -> Vec<Vector2> {
		if points.is_empty() {
			return Vec::new();
		}
		let mut closed_points = points.to_vec();
		closed_points.push(points[0]);
		let mut total_length = 0.0;
		for point_index in 1..closed_points.len() {
			total_length += closed_points[point_index].distance(closed_points[point_index - 1]);
		}
		let target_length = total_length * ratio;
		let mut partial_points = vec![closed_points[0]];
		let mut walked_length = 0.0;
		for point_index in 1..closed_points.len() {
			let from = closed_points[point_index - 1];
			let to = closed_points[point_index];
			let segment_length = to.distance(from);
			if walked_length + segment_length >= target_length {
				let segment_ratio = if segment_length > 0.0 { (target_length - walked_length) / segment_length } else { 0.0 };
				partial_points.push(Vector2::create(from.x + (to.x - from.x) * segment_ratio, from.y + (to.y - from.y) * segment_ratio));
				return partial_points;
			}
			partial_points.push(to);
			walked_length += segment_length;
		}
		partial_points
	}

	//==============================================================================
	/// 채운 몫 설정. (0 ~ 1 로 자른다)
	///
	/// # 인자
	/// - `ratio`: 채운 몫. (0 ~ 1, 넘으면 자른다)
	//==============================================================================
	pub fn set_ratio(&mut self, ratio: f32) {
		self.ratio = ratio.clamp(0.0, 1.0);
	}

	//==============================================================================
	/// 채운 몫 반환.
	///
	/// # 돌려줌
	/// 채운 몫. (0 ~ 1, 기본 0)
	//==============================================================================
	pub fn get_ratio(&self) -> f32 {
		self.ratio
	}

	//==============================================================================
	/// 귀퉁이 둥근 정도 설정. (크기의 반 이상이면 동그라미)
	///
	/// # 인자
	/// - `round_size`: 바깥 꼴의 귀퉁이 반지름. (선의 가운데는 굵기의 반을 뺀 반지름으로 돈다)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f32) {
		self.round_size = round_size;
	}

	//==============================================================================
	/// 귀퉁이 둥근 정도 반환.
	///
	/// # 돌려줌
	/// 바깥 꼴의 귀퉁이 반지름. (기본 0)
	//==============================================================================
	pub fn get_round_size(&self) -> f32 {
		self.round_size
	}

	//==============================================================================
	/// 선 굵기 설정.
	///
	/// # 인자
	/// - `line_width`: 선 굵기. (기본 2)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f32) {
		self.line_width = line_width;
	}

	//==============================================================================
	/// 선 굵기 반환.
	///
	/// # 돌려줌
	/// 선 굵기. (기본 2)
	//==============================================================================
	pub fn get_line_width(&self) -> f32 {
		self.line_width
	}

	//==============================================================================
	/// 채운 몫의 색 설정.
	///
	/// # 인자
	/// - `color`: 채운 몫의 색. (기본 흰색)
	//==============================================================================
	pub fn set_fill_color(&mut self, color: impl Into<Color>) {
		self.fill_color = color.into();
	}

	//==============================================================================
	/// 채운 몫의 색 반환.
	///
	/// # 돌려줌
	/// 채운 몫의 색.
	//==============================================================================
	pub fn get_fill_color(&self) -> Color {
		self.fill_color
	}

	//==============================================================================
	/// 나머지의 색 설정.
	///
	/// # 인자
	/// - `color`: 바탕 고리의 색. (기본 `Color::create(0.2, 0.2, 0.25, 1.0)`)
	//==============================================================================
	pub fn set_track_color(&mut self, color: impl Into<Color>) {
		self.track_color = color.into();
	}

	//==============================================================================
	/// 나머지의 색 반환.
	///
	/// # 돌려줌
	/// 바탕 고리의 색.
	//==============================================================================
	pub fn get_track_color(&self) -> Color {
		self.track_color
	}

	//==============================================================================
	/// 마지막으로 셈한 둘레의 점. (그린 뒤에 맞는 값, 원본 getPerimeterPoints)
	///
	/// # 돌려줌
	/// 선의 가운데가 지나는 둘레의 점. (한 번도 그리지 않았으면 빈 목록)
	//==============================================================================
	pub fn get_perimeter_points(&self) -> &[Vector2] {
		&self.perimeter_points
	}
}

impl Default for UIProgressRing {
	//==============================================================================
	/// 기본값. (UIProgressRing::new 와 같다)
	//==============================================================================
	fn default() -> UIProgressRing {
		UIProgressRing::new()
	}
}

#[crate::reflect]
impl Component for UIProgressRing {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 타입 이름. ("UIProgressRing")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIProgressRing"
	}

	//==============================================================================
	/// tick 이 필요한지. (돌 일이 없어 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 컨텐트 안에 그리는지. (true)
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (draw_ring)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_ring(transform, graphic);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 둘레의 반 몫은 위 가운데에서 시계 방향으로 반 바퀴, 아래 가운데에서 끝난다.
	//==============================================================================
	#[test]
	fn partial_points_follow_length() {
		let points = UIProgressRing::read_perimeter_points(0.0, 0.0, 100.0, 100.0, 0.0);
		let half_points = UIProgressRing::read_partial_points(&points, 0.5);
		let last_point = half_points[half_points.len() - 1];

		// 위 가운데에서 시계 방향으로 반 바퀴면 아래 가운데다.
		assert!((last_point.x - 50.0).abs() < 0.01);
		assert!((last_point.y - 100.0).abs() < 0.01);
	}
}
