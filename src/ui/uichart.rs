//==============================================================================
//! 차트. (원본 ui/uichart.js 의 UILineChart, UIBarChart)
//! - UILineChart: 값 목록을 노드 영역에 폴리라인으로 그린다. push_value 로 새 값을 밀어 넣으면 오래된 값이 앞에서 밀려나는 실시간 그래프가 된다.
//!   표본은 최대 표본 수 기준으로 폭 전체에 퍼진다. (아직 덜 찼으면 왼쪽부터 채운다) 값 범위를 정하지 않으면 지금 값들의 최소 / 최대를 쓴다.
//! - UIBarChart: 값 목록을 세로 막대로 그린다. 최댓값을 정하지 않으면 지금 값들의 최대를 쓴다. 강조 번호의 막대는 강조색이다.
//! - 둘 다 그리기만 하는 컴포넌트라 WorldNode::add_component 로 붙여도 된다. (tick 이 없다)
//! - 원본과 다른 것:
//!   - 값 범위 / 최댓값의 "null 이면 자동" 은 `Option<f64>` 다. 강조 번호의 "-1 이면 없음" 은 `Option<usize>` 다.
//!   - get_value_list 는 복사본 대신 슬라이스를 돌려준다.
//!   - 점 / 막대 자리 계산을 calculate_points / calculate_bar_rects 로 떼어 냈다. (그리기와 같은 계산, 시험과 충돌 판정용)
//!   - 라인 차트는 점 버퍼를 들고 다시 써서 매 프레임 할당하지 않는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
/// 라인 차트.
/// - 그리기만 하는 컴포넌트라 tick 이 없다. 노드 content_size 영역에 가로 격자와 폴리라인을 그린다.
///
/// # 예제
/// 값을 밀어 넣어 꺾은선 그리기
/// ```rust
/// use vanilla::{Color, NodeTree, UILineChart, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let chart_id = node_tree.create_child(root_id, "chart");
/// node_tree[chart_id].set_content_size(Vector2::create(300.0, 120.0));
/// let line_chart = node_tree[chart_id].add_component(UILineChart::new());
/// line_chart.set_max_sample_count(60);
/// line_chart.set_value_range(Some(0.0), Some(100.0));
/// line_chart.set_line_color(Color::create(0.3, 0.8, 1.0, 1.0));
/// line_chart.set_line_width(2.0);
/// for sample_index in 0..80 {
/// 	line_chart.push_value(50.0 + (sample_index as f64 * 0.3).sin() * 30.0);
/// }
///
/// assert_eq!(line_chart.get_value_list().len(), 60);
/// let points = line_chart.calculate_points(300.0, 120.0);
/// assert_eq!(points.len(), 60);
/// assert_eq!(points[59].x, 300.0);
/// ```
//==============================================================================
pub struct UILineChart {
	component_base: ComponentBase,
	value_list: Vec<f64>,
	max_sample_count: usize,
	min_value: Option<f64>,
	max_value: Option<f64>,
	line_color: Color,
	grid_color: Color,
	line_width: f64,
	grid_line_count: usize,
	point_buffer: Vec<Vector2>,
}

#[crate::reflect]
impl UILineChart {
	//==============================================================================
	/// 생성. (표본 60 개, 자동 범위, 격자 3 줄)
	/// # 돌려줌
	/// 값이 빈 새 라인 차트. (선 두께 1.5)
	//==============================================================================
	pub fn new() -> UILineChart {
		UILineChart {
			component_base: ComponentBase::new(),
			value_list: Vec::new(),
			max_sample_count: 60,
			min_value: None,
			max_value: None,
			line_color: Color::create(0.42, 0.48, 1.0, 1.0),
			grid_color: Color::create(1.0, 1.0, 1.0, 0.07),
			line_width: 1.5,
			grid_line_count: 3,
			point_buffer: Vec::new(),
		}
	}

	//==============================================================================
	/// 값 하나 추가. (표본 수를 넘기면 앞에서 밀려난다)
	/// # 인자
	/// - `value`: 끝에 더할 값.
	//==============================================================================
	pub fn push_value(&mut self, value: f64) {
		// 범위를 정했으면 범위 밖 값이 그리는 사각형을 넓힐 수 있다. (범위를 정하지 않았으면 값이 늘 내용 영역 안이다)
		if self.min_value.is_some() || self.max_value.is_some() {
			self.component_base.mark_drawn_rect_dirty();
		}
		self.value_list.push(value);
		if self.value_list.len() > self.max_sample_count {
			let excess_count = self.value_list.len() - self.max_sample_count;
			self.value_list.drain(..excess_count);
		}
	}

	//==============================================================================
	/// 그릴 값 범위. (정하지 않은 쪽은 지금 값들의 최소 / 최대)
	//==============================================================================
	fn resolve_value_range(&self) -> (f64, f64) {
		let mut low_value = self.min_value;
		let mut high_value = self.max_value;
		if low_value.is_none() || high_value.is_none() {
			let first_value = self.value_list.first().copied().unwrap_or(0.0);
			let mut auto_low = first_value;
			let mut auto_high = first_value;
			for value in self.value_list.iter().copied() {
				auto_low = auto_low.min(value);
				auto_high = auto_high.max(value);
			}
			if low_value.is_none() {
				low_value = Some(auto_low);
			}
			if high_value.is_none() {
				high_value = Some(auto_high);
			}
		}
		(low_value.unwrap_or(0.0), high_value.unwrap_or(0.0))
	}

	//==============================================================================
	/// 폴리라인 점 채우기. (표본이 둘보다 적으면 비운다. 위아래 2 픽셀 여백)
	//==============================================================================
	fn fill_points(&self, width: f64, height: f64, points: &mut Vec<Vector2>) {
		points.clear();
		let sample_count = self.value_list.len();
		if sample_count < 2 {
			return;
		}
		let (low_value, high_value) = self.resolve_value_range();
		let value_range = (high_value - low_value).max(0.0001);
		let sample_span = (self.max_sample_count - 1) as f64;
		for (sample_index, value) in self.value_list.iter().copied().enumerate() {
			let ratio = sample_index as f64 / sample_span;
			let value_ratio = (value - low_value) / value_range;
			points.push(Vector2::create(ratio * width, height - value_ratio * (height - 4.0) - 2.0));
		}
	}

	//==============================================================================
	/// 폴리라인 점 계산. (그리기와 같은 자리)
	/// - 표본 i 의 x 는 i / (최대 표본 수 - 1) x 너비라 덜 찬 동안은 왼쪽부터 채운다. y 는 값 범위의 최소가 아래, 최대가 위다. (위아래 2 픽셀 여백)
	///
	/// # 인자
	/// - `width`: 그릴 영역 너비. (픽셀)
	/// - `height`: 그릴 영역 높이. (픽셀)
	///
	/// # 돌려줌
	/// 표본마다 점 하나. (영역 왼쪽 위 기준, 픽셀) 표본이 둘보다 적으면 빈 목록.
	///
	/// # 예제
	/// 자동 범위와 고정 범위의 점
	/// ```rust
	/// use vanilla::{UILineChart, Vector2};
	///
	/// let mut line_chart = UILineChart::new();
	/// line_chart.set_max_sample_count(5);
	/// line_chart.push_value(0.0);
	/// assert!(line_chart.calculate_points(100.0, 54.0).is_empty());
	///
	/// line_chart.push_value(10.0);
	/// line_chart.push_value(5.0);
	/// assert_eq!(line_chart.calculate_points(100.0, 54.0), vec![Vector2::create(0.0, 52.0), Vector2::create(25.0, 2.0), Vector2::create(50.0, 27.0)]);
	///
	/// line_chart.set_value_range(Some(0.0), Some(20.0));
	/// assert_eq!(line_chart.calculate_points(100.0, 54.0)[1], Vector2::create(25.0, 27.0));
	/// ```
	//==============================================================================
	pub fn calculate_points(&self, width: f64, height: f64) -> Vec<Vector2> {
		let mut points: Vec<Vector2> = Vec::new();
		self.fill_points(width, height, &mut points);
		points
	}

	//==============================================================================
	/// 최대 표본 수 설정. (2 보다 작게 하지 않는다. 이미 든 표본이 새 상한보다 많으면 오래된 것부터 버린다)
	///
	/// # 인자
	/// - `max_sample_count`: 들고 그릴 표본 수 상한. (기본 60, 2 보다 작으면 2)
	//==============================================================================
	pub fn set_max_sample_count(&mut self, max_sample_count: usize) {
		self.component_base.mark_drawn_rect_dirty();
		self.max_sample_count = max_sample_count.max(2);
		if self.value_list.len() > self.max_sample_count {
			let excess_count = self.value_list.len() - self.max_sample_count;
			self.value_list.drain(..excess_count);
		}
	}

	//==============================================================================
	/// 최대 표본 수 반환.
	/// # 돌려줌
	/// 표본 수 상한. (2 이상)
	//==============================================================================
	pub fn get_max_sample_count(&self) -> usize {
		self.max_sample_count
	}

	//==============================================================================
	/// 값 범위 설정. (None 이면 그쪽은 자동)
	/// # 인자
	/// - `min_value`: 영역 아래 끝에 올 값. None 이면 지금 값들의 최소.
	/// - `max_value`: 영역 위 끝에 올 값. None 이면 지금 값들의 최대.
	//==============================================================================
	pub fn set_value_range(&mut self, min_value: Option<f64>, max_value: Option<f64>) {
		self.component_base.mark_drawn_rect_dirty();
		self.min_value = min_value;
		self.max_value = max_value;
	}

	//==============================================================================
	/// 값 목록 설정. (최대 표본 수만큼 뒤쪽만 남긴다)
	/// # 인자
	/// - `value_list`: 새 값 목록. (복사해 든다)
	//==============================================================================
	pub fn set_value_list(&mut self, value_list: &[f64]) {
		self.component_base.mark_drawn_rect_dirty();
		let start_index = value_list.len().saturating_sub(self.max_sample_count);
		self.value_list = value_list[start_index..].to_vec();
	}

	//==============================================================================
	/// 값 목록 반환.
	/// # 돌려줌
	/// 지금 든 값. (든 차례대로)
	//==============================================================================
	pub fn get_value_list(&self) -> &[f64] {
		&self.value_list
	}

	//==============================================================================
	/// 선 색 설정.
	/// # 인자
	/// - `line_color`: 폴리라인 색. (기본 (0.42, 0.48, 1, 1))
	//==============================================================================
	pub fn set_line_color(&mut self, line_color: impl Into<Color>) {
		self.line_color = line_color.into();
	}

	//==============================================================================
	/// 격자 색 설정.
	/// # 인자
	/// - `grid_color`: 가로 격자선 색. (기본 (1, 1, 1, 0.07))
	//==============================================================================
	pub fn set_grid_color(&mut self, grid_color: impl Into<Color>) {
		self.grid_color = grid_color.into();
	}

	//==============================================================================
	/// 선 두께 설정.
	/// # 인자
	/// - `line_width`: 폴리라인 두께. (픽셀, 기본 1.5)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f64) {
		self.component_base.mark_drawn_rect_dirty();
		self.line_width = line_width;
	}
}

impl Default for UILineChart {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> UILineChart {
		UILineChart::new()
	}
}

#[crate::reflect]
impl Component for UILineChart {
	//==============================================================================
	/// 컴포넌트 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름. ("UILineChart")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UILineChart"
	}

	//==============================================================================
	/// tick 이 필요한지. (그리기만 해서 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 그리는 사각형. (원본 UILineChart.calculateDrawnRect)
	/// - 내용 영역에 선 두께만큼 넓힌 것. 값 범위를 정했으면 범위 밖 값이 놓이는 높이까지. (범위 밖 값의 선은 내용 영역 밖에 그린다)
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, _graphic: &mut Graphic) -> Option<Rect> {
		let content_size = transform.get_content_size();
		let width = content_size.x;
		let height = content_size.y;
		let mut minimum_y = height.min(0.0);
		let mut maximum_y = height.max(0.0);
		if self.value_list.len() >= 2 && (self.min_value.is_some() || self.max_value.is_some()) {
			let mut low_value = self.value_list[0];
			let mut high_value = self.value_list[0];
			for value in self.value_list.iter().copied() {
				low_value = low_value.min(value);
				high_value = high_value.max(value);
			}
			let range_low_value = self.min_value.unwrap_or(low_value);
			let range_high_value = self.max_value.unwrap_or(high_value);
			let value_range = (range_high_value - range_low_value).max(0.0001);
			let low_y = height - (low_value - range_low_value) / value_range * (height - 4.0) - 2.0;
			let high_y = height - (high_value - range_low_value) / value_range * (height - 4.0) - 2.0;
			minimum_y = minimum_y.min(low_y).min(high_y);
			maximum_y = maximum_y.max(low_y).max(high_y);
		}
		let margin = self.line_width.max(1.0) + 1.0;
		Some(Rect::create(width.min(0.0) - margin, minimum_y - margin, width.abs() + margin * 2.0, maximum_y - minimum_y + margin * 2.0))
	}

	//==============================================================================
	/// 출력. (가로 격자 → 폴리라인)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let width = content_size.x;
		let height = content_size.y;

		// 격자.
		graphic.set_stroke_color(self.grid_color);
		for grid_index in 1..=self.grid_line_count {
			let grid_y = height * grid_index as f64 / (self.grid_line_count + 1) as f64;
			graphic.draw_line(&[Vector2::create(0.0, grid_y), Vector2::create(width, grid_y)], 1.0);
		}

		// 폴리라인. (점 버퍼를 다시 쓴다)
		let mut points = std::mem::take(&mut self.point_buffer);
		self.fill_points(width, height, &mut points);
		if points.len() >= 2 {
			graphic.set_stroke_color(self.line_color);
			graphic.draw_line(&points, self.line_width);
		}
		self.point_buffer = points;
	}
}

//==============================================================================
/// 바 차트.
/// - 그리기만 하는 컴포넌트라 tick 이 없다. 노드 content_size 영역에 값마다 세로 막대를 바닥에서 위로 그린다.
///
/// # 예제
/// 네 값의 막대와 강조
/// ```rust
/// use vanilla::{NodeTree, Rect, UIBarChart, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let chart_id = node_tree.create_child(root_id, "chart");
/// node_tree[chart_id].set_content_size(Vector2::create(100.0, 80.0));
/// let bar_chart = node_tree[chart_id].add_component(UIBarChart::new());
/// bar_chart.set_value_list(&[1.0, 2.0, 4.0, 0.0]);
/// bar_chart.set_highlight_index(Some(2));
///
/// let bar_rects = bar_chart.calculate_bar_rects(100.0, 80.0);
/// assert_eq!(bar_rects[0], Rect::create(0.0, 60.0, 22.0, 20.0));
/// assert_eq!(bar_rects[2], Rect::create(52.0, 0.0, 22.0, 80.0));
/// assert_eq!(bar_rects[3].get_height(), 1.0);
/// ```
//==============================================================================
pub struct UIBarChart {
	component_base: ComponentBase,
	value_list: Vec<f64>,
	max_value: Option<f64>,
	bar_color: Color,
	highlight_color: Color,
	highlight_index: Option<usize>,
	bar_gap: f64,
}

#[crate::reflect]
impl UIBarChart {
	//==============================================================================
	/// 생성. (자동 최댓값, 강조 없음, 막대 사이 4 픽셀)
	/// # 돌려줌
	/// 값이 빈 새 바 차트.
	//==============================================================================
	pub fn new() -> UIBarChart {
		UIBarChart {
			component_base: ComponentBase::new(),
			value_list: Vec::new(),
			max_value: None,
			bar_color: Color::create(0.42, 0.48, 1.0, 0.85),
			highlight_color: Color::create(0.22, 0.84, 1.0, 1.0),
			highlight_index: None,
			bar_gap: 4.0,
		}
	}

	//==============================================================================
	/// 그릴 최댓값. (정하지 않았으면 지금 값들의 최대, 적어도 0.0001)
	//==============================================================================
	fn resolve_max_value(&self) -> f64 {
		match self.max_value {
			Some(max_value) => {
				max_value
			}
			None => {
				self.value_list.iter().copied().fold(0.0001, f64::max)
			}
		}
	}

	//==============================================================================
	/// 막대 하나의 사각형. (바닥에서 위로, 적어도 1 픽셀)
	//==============================================================================
	fn calculate_bar_rect(&self, bar_index: usize, bar_width: f64, height: f64, high_value: f64) -> Rect {
		let value_ratio = (self.value_list[bar_index] / high_value).clamp(0.0, 1.0);
		let bar_height = (value_ratio * height).max(1.0);
		let bar_x = bar_index as f64 * (bar_width + self.bar_gap);
		Rect::create(bar_x, height - bar_height, bar_width, bar_height)
	}

	//==============================================================================
	/// 막대 폭. (막대 사이 간격을 뺀 폭을 똑같이 나눈다)
	//==============================================================================
	fn calculate_bar_width(&self, width: f64) -> f64 {
		let bar_count = self.value_list.len();
		(width - self.bar_gap * (bar_count as f64 - 1.0)) / bar_count as f64
	}

	//==============================================================================
	/// 막대 사각형 목록 계산. (그리기와 같은 자리)
	/// - 막대 폭은 (너비 - 간격 x (막대 수 - 1)) / 막대 수, 높이는 값 / 최댓값 비율(0 ~ 1 로 자름) x 높이이고 적어도 1 픽셀이다.
	///
	/// # 인자
	/// - `width`: 그릴 영역 너비. (픽셀)
	/// - `height`: 그릴 영역 높이. (픽셀)
	///
	/// # 돌려줌
	/// 값마다 막대 사각형. (영역 왼쪽 위 기준, 픽셀) 값이 없으면 빈 목록.
	///
	/// # 예제
	/// 최댓값을 정하면 그 값이 영역 높이다
	/// ```rust
	/// use vanilla::UIBarChart;
	///
	/// let mut bar_chart = UIBarChart::new();
	/// bar_chart.set_value_list(&[1.0, 2.0, 4.0, 0.0]);
	/// assert_eq!(bar_chart.calculate_bar_rects(100.0, 80.0)[2].get_height(), 80.0);
	///
	/// bar_chart.set_max_value(Some(8.0));
	/// assert_eq!(bar_chart.calculate_bar_rects(100.0, 80.0)[2].get_height(), 40.0);
	/// bar_chart.set_max_value(Some(2.0));
	/// assert_eq!(bar_chart.calculate_bar_rects(100.0, 80.0)[2].get_height(), 80.0);
	/// ```
	//==============================================================================
	pub fn calculate_bar_rects(&self, width: f64, height: f64) -> Vec<Rect> {
		if self.value_list.is_empty() {
			return Vec::new();
		}
		let high_value = self.resolve_max_value();
		let bar_width = self.calculate_bar_width(width);
		(0..self.value_list.len()).map(|bar_index| self.calculate_bar_rect(bar_index, bar_width, height, high_value)).collect()
	}

	//==============================================================================
	/// 값 목록 설정.
	/// # 인자
	/// - `value_list`: 막대로 그릴 값 목록. (복사해 든다)
	//==============================================================================
	pub fn set_value_list(&mut self, value_list: &[f64]) {
		self.component_base.mark_drawn_rect_dirty();
		self.value_list = value_list.to_vec();
	}

	//==============================================================================
	/// 값 목록 반환.
	/// # 돌려줌
	/// 지금 든 값. (든 차례대로)
	//==============================================================================
	pub fn get_value_list(&self) -> &[f64] {
		&self.value_list
	}

	//==============================================================================
	/// 최댓값 설정. (None 이면 자동)
	/// # 인자
	/// - `max_value`: 영역 높이에 맞출 값. None 이면 지금 값들의 최대. (적어도 0.0001)
	//==============================================================================
	pub fn set_max_value(&mut self, max_value: Option<f64>) {
		self.max_value = max_value;
	}

	//==============================================================================
	/// 막대 색 설정.
	/// # 인자
	/// - `bar_color`: 막대 색. (기본 (0.42, 0.48, 1, 0.85))
	//==============================================================================
	pub fn set_bar_color(&mut self, bar_color: impl Into<Color>) {
		self.bar_color = bar_color.into();
	}

	//==============================================================================
	/// 강조색 설정.
	/// # 인자
	/// - `highlight_color`: 강조 번호 막대의 색. (기본 (0.22, 0.84, 1, 1))
	//==============================================================================
	pub fn set_highlight_color(&mut self, highlight_color: impl Into<Color>) {
		self.highlight_color = highlight_color.into();
	}

	//==============================================================================
	/// 강조 번호 설정. (None 이면 강조 없음)
	/// # 인자
	/// - `highlight_index`: 강조색으로 그릴 막대 번호. (0 부터)
	//==============================================================================
	pub fn set_highlight_index(&mut self, highlight_index: Option<usize>) {
		self.highlight_index = highlight_index;
	}

	//==============================================================================
	/// 막대 사이 간격 설정.
	/// # 인자
	/// - `bar_gap`: 막대 사이 간격. (픽셀, 기본 4)
	//==============================================================================
	pub fn set_bar_gap(&mut self, bar_gap: f64) {
		self.component_base.mark_drawn_rect_dirty();
		self.bar_gap = bar_gap;
	}
}

impl Default for UIBarChart {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> UIBarChart {
		UIBarChart::new()
	}
}

#[crate::reflect]
impl Component for UIBarChart {
	//==============================================================================
	/// 컴포넌트 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름. ("UIBarChart")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIBarChart"
	}

	//==============================================================================
	/// tick 이 필요한지. (그리기만 해서 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 그리는 사각형. (원본 UIBarChart.calculateDrawnRect)
	/// - 내용 영역. 막대 폭이 음수가 되면 옆으로, 높이가 1 보다 낮으면 위로(막대는 적어도 1 픽셀) 넘친다. 한 단위씩 넓힌다.
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, _graphic: &mut Graphic) -> Option<Rect> {
		let bar_count = self.value_list.len();
		if bar_count == 0 {
			return Some(Rect::create(0.0, 0.0, 0.0, 0.0));
		}
		let content_size = transform.get_content_size();
		let width = content_size.x;
		let height = content_size.y;
		let bar_width = self.calculate_bar_width(width);
		let minimum_x = 0.0_f64.min(bar_width).min(width);
		let maximum_x = width.max(width - bar_width).max(0.0);
		let minimum_y = 0.0_f64.min(height - 1.0);
		let maximum_y = height.max(0.0);
		Some(Rect::create(minimum_x - 1.0, minimum_y - 1.0, maximum_x - minimum_x + 2.0, maximum_y - minimum_y + 2.0))
	}

	//==============================================================================
	/// 출력. (막대마다 사각형)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.value_list.is_empty() {
			return;
		}
		let content_size = transform.get_content_size();
		let high_value = self.resolve_max_value();
		let bar_width = self.calculate_bar_width(content_size.x);
		for bar_index in 0..self.value_list.len() {
			let bar_rect = self.calculate_bar_rect(bar_index, bar_width, content_size.y, high_value);
			let bar_color = if Some(bar_index) == self.highlight_index {
				self.highlight_color
			}
			else {
				self.bar_color
			};
			graphic.set_fill_color(bar_color);
			graphic.draw_rect(bar_rect);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 라인 차트: 표본은 최대 표본 수 기준으로 퍼지고, 자동 / 고정 범위로 높이가 정해진다. 넘친 값은 앞에서 밀려난다.
	//==============================================================================
	#[test]
	fn line_chart_maps_values_to_points() {
		let mut line_chart = UILineChart::new();
		line_chart.set_max_sample_count(5);
		line_chart.push_value(0.0);
		assert!(line_chart.calculate_points(100.0, 54.0).is_empty());
		line_chart.push_value(10.0);
		line_chart.push_value(5.0);
		let points = line_chart.calculate_points(100.0, 54.0);
		assert_eq!(points, vec![Vector2::create(0.0, 52.0), Vector2::create(25.0, 2.0), Vector2::create(50.0, 27.0)]);

		line_chart.set_value_range(Some(0.0), Some(20.0));
		let fixed_points = line_chart.calculate_points(100.0, 54.0);
		assert_eq!(fixed_points[1], Vector2::create(25.0, 27.0));

		for value in [1.0, 2.0, 3.0, 4.0] {
			line_chart.push_value(value);
		}
		assert_eq!(line_chart.get_value_list(), &[5.0, 1.0, 2.0, 3.0, 4.0]);
		line_chart.set_value_list(&[9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0]);
		assert_eq!(line_chart.get_value_list(), &[7.0, 6.0, 5.0, 4.0, 3.0]);
		line_chart.set_max_sample_count(1);
		assert_eq!(line_chart.get_max_sample_count(), 2);
	}

	//==============================================================================
	/// 라인 차트: 최대 표본 수를 줄이면 오래된 표본부터 버려, 점이 그래프 폭을 넘지 않는다.
	//==============================================================================
	#[test]
	fn line_chart_max_sample_count_trims_oldest() {
		let mut line_chart = UILineChart::new();
		line_chart.set_value_list(&[1.0, 2.0, 3.0, 4.0, 5.0]);
		line_chart.set_max_sample_count(3);
		assert_eq!(line_chart.get_value_list(), &[3.0, 4.0, 5.0]);
		let points = line_chart.calculate_points(100.0, 54.0);
		assert_eq!(points.len(), 3);
		assert_eq!(points[2].x, 100.0);
		line_chart.set_max_sample_count(1);
		assert_eq!(line_chart.get_value_list(), &[4.0, 5.0]);
		line_chart.set_max_sample_count(10);
		assert_eq!(line_chart.get_value_list(), &[4.0, 5.0]);
	}

	//==============================================================================
	/// 바 차트: 간격을 뺀 폭을 나누고, 최댓값 대비 높이로 바닥에서 올린다. (적어도 1 픽셀)
	//==============================================================================
	#[test]
	fn bar_chart_maps_values_to_rects() {
		let mut bar_chart = UIBarChart::new();
		assert!(bar_chart.calculate_bar_rects(100.0, 80.0).is_empty());
		bar_chart.set_value_list(&[1.0, 2.0, 4.0, 0.0]);
		let bar_rects = bar_chart.calculate_bar_rects(100.0, 80.0);
		assert_eq!(bar_rects.len(), 4);
		let bar_width = (100.0 - 12.0) / 4.0;
		assert_eq!(bar_rects[0], Rect::create(0.0, 60.0, bar_width, 20.0));
		assert_eq!(bar_rects[2], Rect::create(2.0 * (bar_width + 4.0), 0.0, bar_width, 80.0));
		assert_eq!(bar_rects[3].get_height(), 1.0);

		bar_chart.set_max_value(Some(8.0));
		let fixed_rects = bar_chart.calculate_bar_rects(100.0, 80.0);
		assert_eq!(fixed_rects[2].get_height(), 40.0);
		bar_chart.set_max_value(Some(2.0));
		assert_eq!(bar_chart.calculate_bar_rects(100.0, 80.0)[2].get_height(), 80.0);
	}
}
