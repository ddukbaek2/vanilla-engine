//==============================================================================
//! 차트. (원본 ui/uichart.js 의 UILineChart, UIBarChart)
//! - UILineChart: 값 목록을 노드 영역에 폴리라인으로 그린다. push_value 로 새 값을 밀어 넣으면 오래된 값이 앞에서 밀려나는 실시간 그래프가 된다.
//!   표본은 최대 표본 수 기준으로 폭 전체에 퍼진다. (아직 덜 찼으면 왼쪽부터 채운다) 값 범위를 정하지 않으면 지금 값들의 최소 / 최대를 쓴다.
//! - UIBarChart: 값 목록을 세로 막대로 그린다. 최댓값을 정하지 않으면 지금 값들의 최대를 쓴다. 강조 번호의 막대는 강조색이다.
//! - 둘 다 그리기만 하는 컴포넌트라 WorldNode::add_component 로 붙여도 된다. (tick 이 없다)
//! - 원본과 다른 것:
//!   - 값 범위 / 최댓값의 "null 이면 자동" 은 `Option<f32>` 다. 강조 번호의 "-1 이면 없음" 은 `Option<usize>` 다.
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
//==============================================================================
pub struct UILineChart {
	component_base: ComponentBase,
	value_list: Vec<f32>,
	max_sample_count: usize,
	min_value: Option<f32>,
	max_value: Option<f32>,
	line_color: Color,
	grid_color: Color,
	line_width: f32,
	grid_line_count: usize,
	point_buffer: Vec<Vector2>,
}

#[crate::reflect]
impl UILineChart {
	//==============================================================================
	/// 생성. (표본 60 개, 자동 범위, 격자 3 줄)
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
	//==============================================================================
	pub fn push_value(&mut self, value: f32) {
		self.value_list.push(value);
		if self.value_list.len() > self.max_sample_count {
			let excess_count = self.value_list.len() - self.max_sample_count;
			self.value_list.drain(..excess_count);
		}
	}

	//==============================================================================
	/// 그릴 값 범위. (정하지 않은 쪽은 지금 값들의 최소 / 최대)
	//==============================================================================
	fn resolve_value_range(&self) -> (f32, f32) {
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
	fn fill_points(&self, width: f32, height: f32, points: &mut Vec<Vector2>) {
		points.clear();
		let sample_count = self.value_list.len();
		if sample_count < 2 {
			return;
		}
		let (low_value, high_value) = self.resolve_value_range();
		let value_range = (high_value - low_value).max(0.0001);
		let sample_span = (self.max_sample_count - 1) as f32;
		for (sample_index, value) in self.value_list.iter().copied().enumerate() {
			let ratio = sample_index as f32 / sample_span;
			let value_ratio = (value - low_value) / value_range;
			points.push(Vector2::create(ratio * width, height - value_ratio * (height - 4.0) - 2.0));
		}
	}

	//==============================================================================
	/// 폴리라인 점 계산. (그리기와 같은 자리)
	//==============================================================================
	pub fn calculate_points(&self, width: f32, height: f32) -> Vec<Vector2> {
		let mut points: Vec<Vector2> = Vec::new();
		self.fill_points(width, height, &mut points);
		points
	}

	//==============================================================================
	/// 최대 표본 수 설정. (2 보다 작게 하지 않는다. 이미 든 표본이 새 상한보다 많으면 오래된 것부터 버린다)
	//==============================================================================
	pub fn set_max_sample_count(&mut self, max_sample_count: usize) {
		self.max_sample_count = max_sample_count.max(2);
		if self.value_list.len() > self.max_sample_count {
			let excess_count = self.value_list.len() - self.max_sample_count;
			self.value_list.drain(..excess_count);
		}
	}

	//==============================================================================
	/// 최대 표본 수 반환.
	//==============================================================================
	pub fn get_max_sample_count(&self) -> usize {
		self.max_sample_count
	}

	//==============================================================================
	/// 값 범위 설정. (None 이면 그쪽은 자동)
	//==============================================================================
	pub fn set_value_range(&mut self, min_value: Option<f32>, max_value: Option<f32>) {
		self.min_value = min_value;
		self.max_value = max_value;
	}

	//==============================================================================
	/// 값 목록 설정. (최대 표본 수만큼 뒤쪽만 남긴다)
	//==============================================================================
	pub fn set_value_list(&mut self, value_list: &[f32]) {
		let start_index = value_list.len().saturating_sub(self.max_sample_count);
		self.value_list = value_list[start_index..].to_vec();
	}

	//==============================================================================
	/// 값 목록 반환.
	//==============================================================================
	pub fn get_value_list(&self) -> &[f32] {
		&self.value_list
	}

	//==============================================================================
	/// 선 색 설정.
	//==============================================================================
	pub fn set_line_color(&mut self, line_color: impl Into<Color>) {
		self.line_color = line_color.into();
	}

	//==============================================================================
	/// 격자 색 설정.
	//==============================================================================
	pub fn set_grid_color(&mut self, grid_color: impl Into<Color>) {
		self.grid_color = grid_color.into();
	}

	//==============================================================================
	/// 선 두께 설정.
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f32) {
		self.line_width = line_width;
	}
}

impl Default for UILineChart {
	fn default() -> UILineChart {
		UILineChart::new()
	}
}

#[crate::reflect]
impl Component for UILineChart {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UILineChart"
	}

	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
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
			let grid_y = height * grid_index as f32 / (self.grid_line_count + 1) as f32;
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
//==============================================================================
pub struct UIBarChart {
	component_base: ComponentBase,
	value_list: Vec<f32>,
	max_value: Option<f32>,
	bar_color: Color,
	highlight_color: Color,
	highlight_index: Option<usize>,
	bar_gap: f32,
}

#[crate::reflect]
impl UIBarChart {
	//==============================================================================
	/// 생성. (자동 최댓값, 강조 없음, 막대 사이 4 픽셀)
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
	fn resolve_max_value(&self) -> f32 {
		match self.max_value {
			Some(max_value) => {
				max_value
			}
			None => {
				self.value_list.iter().copied().fold(0.0001, f32::max)
			}
		}
	}

	//==============================================================================
	/// 막대 하나의 사각형. (바닥에서 위로, 적어도 1 픽셀)
	//==============================================================================
	fn calculate_bar_rect(&self, bar_index: usize, bar_width: f32, height: f32, high_value: f32) -> Rect {
		let value_ratio = (self.value_list[bar_index] / high_value).clamp(0.0, 1.0);
		let bar_height = (value_ratio * height).max(1.0);
		let bar_x = bar_index as f32 * (bar_width + self.bar_gap);
		Rect::create(bar_x, height - bar_height, bar_width, bar_height)
	}

	//==============================================================================
	/// 막대 폭. (막대 사이 간격을 뺀 폭을 똑같이 나눈다)
	//==============================================================================
	fn calculate_bar_width(&self, width: f32) -> f32 {
		let bar_count = self.value_list.len();
		(width - self.bar_gap * (bar_count as f32 - 1.0)) / bar_count as f32
	}

	//==============================================================================
	/// 막대 사각형 목록 계산. (그리기와 같은 자리)
	//==============================================================================
	pub fn calculate_bar_rects(&self, width: f32, height: f32) -> Vec<Rect> {
		if self.value_list.is_empty() {
			return Vec::new();
		}
		let high_value = self.resolve_max_value();
		let bar_width = self.calculate_bar_width(width);
		(0..self.value_list.len()).map(|bar_index| self.calculate_bar_rect(bar_index, bar_width, height, high_value)).collect()
	}

	//==============================================================================
	/// 값 목록 설정.
	//==============================================================================
	pub fn set_value_list(&mut self, value_list: &[f32]) {
		self.value_list = value_list.to_vec();
	}

	//==============================================================================
	/// 값 목록 반환.
	//==============================================================================
	pub fn get_value_list(&self) -> &[f32] {
		&self.value_list
	}

	//==============================================================================
	/// 최댓값 설정. (None 이면 자동)
	//==============================================================================
	pub fn set_max_value(&mut self, max_value: Option<f32>) {
		self.max_value = max_value;
	}

	//==============================================================================
	/// 막대 색 설정.
	//==============================================================================
	pub fn set_bar_color(&mut self, bar_color: impl Into<Color>) {
		self.bar_color = bar_color.into();
	}

	//==============================================================================
	/// 강조색 설정.
	//==============================================================================
	pub fn set_highlight_color(&mut self, highlight_color: impl Into<Color>) {
		self.highlight_color = highlight_color.into();
	}

	//==============================================================================
	/// 강조 번호 설정. (None 이면 강조 없음)
	//==============================================================================
	pub fn set_highlight_index(&mut self, highlight_index: Option<usize>) {
		self.highlight_index = highlight_index;
	}

	//==============================================================================
	/// 막대 사이 간격 설정.
	//==============================================================================
	pub fn set_bar_gap(&mut self, bar_gap: f32) {
		self.bar_gap = bar_gap;
	}
}

impl Default for UIBarChart {
	fn default() -> UIBarChart {
		UIBarChart::new()
	}
}

#[crate::reflect]
impl Component for UIBarChart {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIBarChart"
	}

	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
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
