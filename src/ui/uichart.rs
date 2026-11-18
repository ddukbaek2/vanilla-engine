//==============================================================================
// 차트. (원본 ui/uichart.js 의 UILineChart, UIBarChart)
// - UILineChart: 값 목록을 노드 영역에 폴리라인으로 그린다. pushValue 로 새 값을 밀어 넣으면 오래된 값이 앞에서 밀려나는 실시간 그래프가 된다.
//   표본은 최대 표본 수 기준으로 폭 전체에 퍼진다. (아직 덜 찼으면 왼쪽부터 채운다) 값 범위를 정하지 않으면 지금 값들의 최소 / 최대를 쓴다.
// - UIBarChart: 값 목록을 세로 막대로 그린다. 최댓값을 정하지 않으면 지금 값들의 최대를 쓴다. 강조 번호의 막대는 강조색이다.
// - 둘 다 그리기만 하는 컴포넌트라 WorldNode::addComponent 로 붙여도 된다. (tick 이 없다)
// - 원본과 다른 것:
//   - 값 범위 / 최댓값의 "null 이면 자동" 은 Option<f32> 다. 강조 번호의 "-1 이면 없음" 은 Option<usize> 다.
//   - getValueList 는 복사본 대신 슬라이스를 돌려준다.
//   - 점 / 막대 자리 계산을 calculatePoints / calculateBarRects 로 떼어 냈다. (그리기와 같은 계산, 시험과 충돌 판정용)
//   - 라인 차트는 점 버퍼를 들고 다시 써서 매 프레임 할당하지 않는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
// 라인 차트.
//==============================================================================
pub struct UILineChart {
	componentBase: ComponentBase,
	valueList: Vec<f32>,
	maxSampleCount: usize,
	minValue: Option<f32>,
	maxValue: Option<f32>,
	lineColor: Color,
	gridColor: Color,
	lineWidth: f32,
	gridLineCount: usize,
	pointBuffer: Vec<Vector2>,
}

impl UILineChart {
	//==============================================================================
	// 생성. (표본 60 개, 자동 범위, 격자 3 줄)
	//==============================================================================
	pub fn new() -> UILineChart {
		UILineChart {
			componentBase: ComponentBase::new(),
			valueList: Vec::new(),
			maxSampleCount: 60,
			minValue: None,
			maxValue: None,
			lineColor: Color::create(0.42, 0.48, 1.0, 1.0),
			gridColor: Color::create(1.0, 1.0, 1.0, 0.07),
			lineWidth: 1.5,
			gridLineCount: 3,
			pointBuffer: Vec::new(),
		}
	}

	//==============================================================================
	// 값 하나 추가. (표본 수를 넘기면 앞에서 밀려난다)
	//==============================================================================
	pub fn pushValue(&mut self, value: f32) {
		self.valueList.push(value);
		if self.valueList.len() > self.maxSampleCount {
			let excessCount = self.valueList.len() - self.maxSampleCount;
			self.valueList.drain(..excessCount);
		}
	}

	//==============================================================================
	// 그릴 값 범위. (정하지 않은 쪽은 지금 값들의 최소 / 최대)
	//==============================================================================
	fn resolveValueRange(&self) -> (f32, f32) {
		let mut lowValue = self.minValue;
		let mut highValue = self.maxValue;
		if lowValue.is_none() || highValue.is_none() {
			let firstValue = self.valueList.first().copied().unwrap_or(0.0);
			let mut autoLow = firstValue;
			let mut autoHigh = firstValue;
			for value in self.valueList.iter().copied() {
				autoLow = autoLow.min(value);
				autoHigh = autoHigh.max(value);
			}
			if lowValue.is_none() {
				lowValue = Some(autoLow);
			}
			if highValue.is_none() {
				highValue = Some(autoHigh);
			}
		}
		(lowValue.unwrap_or(0.0), highValue.unwrap_or(0.0))
	}

	//==============================================================================
	// 폴리라인 점 채우기. (표본이 둘보다 적으면 비운다. 위아래 2 픽셀 여백)
	//==============================================================================
	fn fillPoints(&self, width: f32, height: f32, points: &mut Vec<Vector2>) {
		points.clear();
		let sampleCount = self.valueList.len();
		if sampleCount < 2 {
			return;
		}
		let (lowValue, highValue) = self.resolveValueRange();
		let valueRange = (highValue - lowValue).max(0.0001);
		let sampleSpan = (self.maxSampleCount - 1) as f32;
		for (sampleIndex, value) in self.valueList.iter().copied().enumerate() {
			let ratio = sampleIndex as f32 / sampleSpan;
			let valueRatio = (value - lowValue) / valueRange;
			points.push(Vector2::create(ratio * width, height - valueRatio * (height - 4.0) - 2.0));
		}
	}

	//==============================================================================
	// 폴리라인 점 계산. (그리기와 같은 자리)
	//==============================================================================
	pub fn calculatePoints(&self, width: f32, height: f32) -> Vec<Vector2> {
		let mut points: Vec<Vector2> = Vec::new();
		self.fillPoints(width, height, &mut points);
		points
	}

	//==============================================================================
	// 최대 표본 수 설정. (2 보다 작게 하지 않는다)
	//==============================================================================
	pub fn setMaxSampleCount(&mut self, maxSampleCount: usize) {
		self.maxSampleCount = maxSampleCount.max(2);
	}

	//==============================================================================
	// 최대 표본 수 반환.
	//==============================================================================
	pub fn getMaxSampleCount(&self) -> usize {
		self.maxSampleCount
	}

	//==============================================================================
	// 값 범위 설정. (None 이면 그쪽은 자동)
	//==============================================================================
	pub fn setValueRange(&mut self, minValue: Option<f32>, maxValue: Option<f32>) {
		self.minValue = minValue;
		self.maxValue = maxValue;
	}

	//==============================================================================
	// 값 목록 설정. (최대 표본 수만큼 뒤쪽만 남긴다)
	//==============================================================================
	pub fn setValueList(&mut self, valueList: &[f32]) {
		let startIndex = valueList.len().saturating_sub(self.maxSampleCount);
		self.valueList = valueList[startIndex..].to_vec();
	}

	//==============================================================================
	// 값 목록 반환.
	//==============================================================================
	pub fn getValueList(&self) -> &[f32] {
		&self.valueList
	}

	//==============================================================================
	// 선 색 설정.
	//==============================================================================
	pub fn setLineColor(&mut self, lineColor: impl Into<Color>) {
		self.lineColor = lineColor.into();
	}

	//==============================================================================
	// 격자 색 설정.
	//==============================================================================
	pub fn setGridColor(&mut self, gridColor: impl Into<Color>) {
		self.gridColor = gridColor.into();
	}

	//==============================================================================
	// 선 두께 설정.
	//==============================================================================
	pub fn setLineWidth(&mut self, lineWidth: f32) {
		self.lineWidth = lineWidth;
	}
}

impl Default for UILineChart {
	fn default() -> UILineChart {
		UILineChart::new()
	}
}

impl Component for UILineChart {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UILineChart"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (가로 격자 → 폴리라인)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		let width = contentSize.x;
		let height = contentSize.y;

		// 격자.
		graphic.setStrokeColor(self.gridColor);
		for gridIndex in 1..=self.gridLineCount {
			let gridY = height * gridIndex as f32 / (self.gridLineCount + 1) as f32;
			graphic.drawLine(&[Vector2::create(0.0, gridY), Vector2::create(width, gridY)], 1.0);
		}

		// 폴리라인. (점 버퍼를 다시 쓴다)
		let mut points = std::mem::take(&mut self.pointBuffer);
		self.fillPoints(width, height, &mut points);
		if points.len() >= 2 {
			graphic.setStrokeColor(self.lineColor);
			graphic.drawLine(&points, self.lineWidth);
		}
		self.pointBuffer = points;
	}
}

//==============================================================================
// 바 차트.
//==============================================================================
pub struct UIBarChart {
	componentBase: ComponentBase,
	valueList: Vec<f32>,
	maxValue: Option<f32>,
	barColor: Color,
	highlightColor: Color,
	highlightIndex: Option<usize>,
	barGap: f32,
}

impl UIBarChart {
	//==============================================================================
	// 생성. (자동 최댓값, 강조 없음, 막대 사이 4 픽셀)
	//==============================================================================
	pub fn new() -> UIBarChart {
		UIBarChart {
			componentBase: ComponentBase::new(),
			valueList: Vec::new(),
			maxValue: None,
			barColor: Color::create(0.42, 0.48, 1.0, 0.85),
			highlightColor: Color::create(0.22, 0.84, 1.0, 1.0),
			highlightIndex: None,
			barGap: 4.0,
		}
	}

	//==============================================================================
	// 그릴 최댓값. (정하지 않았으면 지금 값들의 최대, 적어도 0.0001)
	//==============================================================================
	fn resolveMaxValue(&self) -> f32 {
		match self.maxValue {
			Some(maxValue) => {
				maxValue
			}
			None => {
				self.valueList.iter().copied().fold(0.0001, f32::max)
			}
		}
	}

	//==============================================================================
	// 막대 하나의 사각형. (바닥에서 위로, 적어도 1 픽셀)
	//==============================================================================
	fn calculateBarRect(&self, barIndex: usize, barWidth: f32, height: f32, highValue: f32) -> Rect {
		let valueRatio = (self.valueList[barIndex] / highValue).clamp(0.0, 1.0);
		let barHeight = (valueRatio * height).max(1.0);
		let barX = barIndex as f32 * (barWidth + self.barGap);
		Rect::create(barX, height - barHeight, barWidth, barHeight)
	}

	//==============================================================================
	// 막대 폭. (막대 사이 간격을 뺀 폭을 똑같이 나눈다)
	//==============================================================================
	fn calculateBarWidth(&self, width: f32) -> f32 {
		let barCount = self.valueList.len();
		(width - self.barGap * (barCount as f32 - 1.0)) / barCount as f32
	}

	//==============================================================================
	// 막대 사각형 목록 계산. (그리기와 같은 자리)
	//==============================================================================
	pub fn calculateBarRects(&self, width: f32, height: f32) -> Vec<Rect> {
		if self.valueList.is_empty() {
			return Vec::new();
		}
		let highValue = self.resolveMaxValue();
		let barWidth = self.calculateBarWidth(width);
		(0..self.valueList.len()).map(|barIndex| self.calculateBarRect(barIndex, barWidth, height, highValue)).collect()
	}

	//==============================================================================
	// 값 목록 설정.
	//==============================================================================
	pub fn setValueList(&mut self, valueList: &[f32]) {
		self.valueList = valueList.to_vec();
	}

	//==============================================================================
	// 값 목록 반환.
	//==============================================================================
	pub fn getValueList(&self) -> &[f32] {
		&self.valueList
	}

	//==============================================================================
	// 최댓값 설정. (None 이면 자동)
	//==============================================================================
	pub fn setMaxValue(&mut self, maxValue: Option<f32>) {
		self.maxValue = maxValue;
	}

	//==============================================================================
	// 막대 색 설정.
	//==============================================================================
	pub fn setBarColor(&mut self, barColor: impl Into<Color>) {
		self.barColor = barColor.into();
	}

	//==============================================================================
	// 강조색 설정.
	//==============================================================================
	pub fn setHighlightColor(&mut self, highlightColor: impl Into<Color>) {
		self.highlightColor = highlightColor.into();
	}

	//==============================================================================
	// 강조 번호 설정. (None 이면 강조 없음)
	//==============================================================================
	pub fn setHighlightIndex(&mut self, highlightIndex: Option<usize>) {
		self.highlightIndex = highlightIndex;
	}

	//==============================================================================
	// 막대 사이 간격 설정.
	//==============================================================================
	pub fn setBarGap(&mut self, barGap: f32) {
		self.barGap = barGap;
	}
}

impl Default for UIBarChart {
	fn default() -> UIBarChart {
		UIBarChart::new()
	}
}

impl Component for UIBarChart {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"UIBarChart"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (막대마다 사각형)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.valueList.is_empty() {
			return;
		}
		let contentSize = transform.getContentSize();
		let highValue = self.resolveMaxValue();
		let barWidth = self.calculateBarWidth(contentSize.x);
		for barIndex in 0..self.valueList.len() {
			let barRect = self.calculateBarRect(barIndex, barWidth, contentSize.y, highValue);
			let barColor = if Some(barIndex) == self.highlightIndex {
				self.highlightColor
			}
			else {
				self.barColor
			};
			graphic.setFillColor(barColor);
			graphic.drawRect(barRect);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 라인 차트: 표본은 최대 표본 수 기준으로 퍼지고, 자동 / 고정 범위로 높이가 정해진다. 넘친 값은 앞에서 밀려난다.
	//==============================================================================
	#[test]
	fn lineChartMapsValuesToPoints() {
		let mut lineChart = UILineChart::new();
		lineChart.setMaxSampleCount(5);
		lineChart.pushValue(0.0);
		assert!(lineChart.calculatePoints(100.0, 54.0).is_empty());
		lineChart.pushValue(10.0);
		lineChart.pushValue(5.0);
		let points = lineChart.calculatePoints(100.0, 54.0);
		assert_eq!(points, vec![Vector2::create(0.0, 52.0), Vector2::create(25.0, 2.0), Vector2::create(50.0, 27.0)]);

		lineChart.setValueRange(Some(0.0), Some(20.0));
		let fixedPoints = lineChart.calculatePoints(100.0, 54.0);
		assert_eq!(fixedPoints[1], Vector2::create(25.0, 27.0));

		for value in [1.0, 2.0, 3.0, 4.0] {
			lineChart.pushValue(value);
		}
		assert_eq!(lineChart.getValueList(), &[5.0, 1.0, 2.0, 3.0, 4.0]);
		lineChart.setValueList(&[9.0, 8.0, 7.0, 6.0, 5.0, 4.0, 3.0]);
		assert_eq!(lineChart.getValueList(), &[7.0, 6.0, 5.0, 4.0, 3.0]);
		lineChart.setMaxSampleCount(1);
		assert_eq!(lineChart.getMaxSampleCount(), 2);
	}

	//==============================================================================
	// 바 차트: 간격을 뺀 폭을 나누고, 최댓값 대비 높이로 바닥에서 올린다. (적어도 1 픽셀)
	//==============================================================================
	#[test]
	fn barChartMapsValuesToRects() {
		let mut barChart = UIBarChart::new();
		assert!(barChart.calculateBarRects(100.0, 80.0).is_empty());
		barChart.setValueList(&[1.0, 2.0, 4.0, 0.0]);
		let barRects = barChart.calculateBarRects(100.0, 80.0);
		assert_eq!(barRects.len(), 4);
		let barWidth = (100.0 - 12.0) / 4.0;
		assert_eq!(barRects[0], Rect::create(0.0, 60.0, barWidth, 20.0));
		assert_eq!(barRects[2], Rect::create(2.0 * (barWidth + 4.0), 0.0, barWidth, 80.0));
		assert_eq!(barRects[3].getHeight(), 1.0);

		barChart.setMaxValue(Some(8.0));
		let fixedRects = barChart.calculateBarRects(100.0, 80.0);
		assert_eq!(fixedRects[2].getHeight(), 40.0);
		barChart.setMaxValue(Some(2.0));
		assert_eq!(barChart.calculateBarRects(100.0, 80.0)[2].getHeight(), 80.0);
	}
}
