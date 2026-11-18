//==============================================================================
// 진행률 표시. (원본 ui/uiprogressview.js 의 UIProgressView, ProgressDirection)
// - 호스트 contentSize 영역을 트랙으로 쓰고, 값 비율만큼 채움을 그린다. 입력은 받지 않는다.
// - 트랙 색은 원본처럼 UIView 의 배경색이다. (setTrackColor 는 setBackgroundColor 의 다른 이름)
// - 반대편 값(secondaryValue)을 정하면 진행 방향의 반대쪽 끝에서 차오르는 두 번째 채움을 그린다. (원본 null 은 None)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
// 진행 방향.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressDirection {
	// 왼쪽에서 오른쪽으로 채움.
	horizontal,
	// 아래에서 위로 채움.
	vertical,
}

//==============================================================================
// 진행률 표시.
//==============================================================================
pub struct UIProgressView {
	uiView: UIView,
	value: f32,
	minValue: f32,
	maxValue: f32,
	fillColor: Color,
	secondaryValue: Option<f32>,
	secondaryFillColor: Color,
	cornerRadius: f32,
	direction: ProgressDirection,
}

impl UIProgressView {
	//==============================================================================
	// 생성. (값 0, 범위 0 ~ 1, 트랙 회색, 채움 파랑)
	//==============================================================================
	pub fn new() -> UIProgressView {
		let mut uiView = UIView::new();
		uiView.setBackgroundColor(Color::create(0.7, 0.7, 0.7, 1.0));
		UIProgressView {
			uiView,
			value: 0.0,
			minValue: 0.0,
			maxValue: 1.0,
			fillColor: Color::create(0.23, 0.51, 0.96, 1.0),
			secondaryValue: None,
			secondaryFillColor: Color::create(0.94, 0.33, 0.31, 1.0),
			cornerRadius: 0.0,
			direction: ProgressDirection::horizontal,
		}
	}

	//==============================================================================
	// 품은 뷰 반환.
	//==============================================================================
	pub fn getUIView(&self) -> &UIView {
		&self.uiView
	}

	//==============================================================================
	// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn getUIViewMut(&mut self) -> &mut UIView {
		&mut self.uiView
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiView.getContent()
	}

	//==============================================================================
	// 배경색 설정. (트랙 색)
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.uiView.setBackgroundColor(backgroundColor);
	}

	//==============================================================================
	// 배경색 반환. (트랙 색)
	//==============================================================================
	pub fn getBackgroundColor(&self) -> Color {
		self.uiView.getBackgroundColor()
	}

	//==============================================================================
	// 사각형 채우기. (둥근 크기가 있으면 둥글게)
	//==============================================================================
	fn fillRect(&self, graphic: &mut Graphic, rect: Rect, color: Color) {
		graphic.setFillColor(color);
		if self.cornerRadius > 0.0 {
			graphic.drawRoundRect(rect, self.cornerRadius);
		}
		else {
			graphic.drawRect(rect);
		}
	}

	//==============================================================================
	// 0 ~ 1 비율 반환.
	//==============================================================================
	pub fn getRatio(&self) -> f32 {
		let range = self.maxValue - self.minValue;
		if range <= 0.0 {
			return 0.0;
		}
		let ratio = (self.value - self.minValue) / range;
		math::clamp(ratio, 0.0, 1.0)
	}

	//==============================================================================
	// 값 설정. (범위로 자른다)
	//==============================================================================
	pub fn setValue(&mut self, value: f32) {
		self.value = math::clamp(value, self.minValue, self.maxValue);
	}

	//==============================================================================
	// 값 반환.
	//==============================================================================
	pub fn getValue(&self) -> f32 {
		self.value
	}

	//==============================================================================
	// 범위 설정. (지금 값을 새 범위로 다시 자른다)
	//==============================================================================
	pub fn setRange(&mut self, minValue: f32, maxValue: f32) {
		self.minValue = minValue;
		self.maxValue = maxValue;
		let value = self.value;
		self.setValue(value);
	}

	//==============================================================================
	// 최소값 반환.
	//==============================================================================
	pub fn getMinValue(&self) -> f32 {
		self.minValue
	}

	//==============================================================================
	// 최대값 반환.
	//==============================================================================
	pub fn getMaxValue(&self) -> f32 {
		self.maxValue
	}

	//==============================================================================
	// 반대편 값 설정. (None 이면 끔. 범위로 자른다)
	//==============================================================================
	pub fn setSecondaryValue(&mut self, secondaryValue: Option<f32>) {
		self.secondaryValue = secondaryValue.map(|value| math::clamp(value, self.minValue, self.maxValue));
	}

	//==============================================================================
	// 반대편 값 반환.
	//==============================================================================
	pub fn getSecondaryValue(&self) -> Option<f32> {
		self.secondaryValue
	}

	//==============================================================================
	// 반대편 채움 색 설정.
	//==============================================================================
	pub fn setSecondaryFillColor(&mut self, color: impl Into<Color>) {
		self.secondaryFillColor = color.into();
	}

	//==============================================================================
	// 반대편 채움 색 반환.
	//==============================================================================
	pub fn getSecondaryFillColor(&self) -> Color {
		self.secondaryFillColor
	}

	//==============================================================================
	// 트랙 색 설정. (배경색)
	//==============================================================================
	pub fn setTrackColor(&mut self, color: impl Into<Color>) {
		self.uiView.setBackgroundColor(color);
	}

	//==============================================================================
	// 트랙 색 반환. (배경색)
	//==============================================================================
	pub fn getTrackColor(&self) -> Color {
		self.uiView.getBackgroundColor()
	}

	//==============================================================================
	// 채움 색 설정.
	//==============================================================================
	pub fn setFillColor(&mut self, color: impl Into<Color>) {
		self.fillColor = color.into();
	}

	//==============================================================================
	// 채움 색 반환.
	//==============================================================================
	pub fn getFillColor(&self) -> Color {
		self.fillColor
	}

	//==============================================================================
	// 둥근 모서리 반지름 설정.
	//==============================================================================
	pub fn setCornerRadius(&mut self, cornerRadius: f32) {
		self.cornerRadius = cornerRadius;
	}

	//==============================================================================
	// 둥근 모서리 반지름 반환.
	//==============================================================================
	pub fn getCornerRadius(&self) -> f32 {
		self.cornerRadius
	}

	//==============================================================================
	// 방향 설정.
	//==============================================================================
	pub fn setDirection(&mut self, direction: ProgressDirection) {
		self.direction = direction;
	}

	//==============================================================================
	// 방향 반환.
	//==============================================================================
	pub fn getDirection(&self) -> ProgressDirection {
		self.direction
	}
}

impl Default for UIProgressView {
	fn default() -> UIProgressView {
		UIProgressView::new()
	}
}

impl Component for UIProgressView {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UIProgressView"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiView.createContentNode(nodeTree, nodeId);
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	//==============================================================================
	// 출력. (트랙 → 채움 → 반대편 채움. 둥근 모서리 때문에 배경을 직접 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		if contentSize.x <= 0.0 || contentSize.y <= 0.0 {
			return;
		}

		// 트랙.
		let trackRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
		let trackColor = self.getBackgroundColor();
		self.fillRect(graphic, trackRect, trackColor);

		// 채움.
		let ratio = self.getRatio();
		if ratio <= 0.0 {
			return;
		}
		let fillRect = match self.direction {
			ProgressDirection::vertical => {
				let fillHeight = contentSize.y * ratio;
				Rect::create(0.0, contentSize.y - fillHeight, contentSize.x, fillHeight)
			}
			ProgressDirection::horizontal => {
				let fillWidth = contentSize.x * ratio;
				Rect::create(0.0, 0.0, fillWidth, contentSize.y)
			}
		};
		let fillColor = self.fillColor;
		self.fillRect(graphic, fillRect, fillColor);

		// 반대편 게이지. (점유율 / 대전 게이지처럼 한 바에 양쪽을 함께 보일 때)
		let secondaryValue = match self.secondaryValue {
			Some(secondaryValue) => {
				secondaryValue
			}
			None => {
				return;
			}
		};
		let range = self.maxValue - self.minValue;
		if range <= 0.0 {
			return;
		}
		let secondaryRatio = math::clamp((secondaryValue - self.minValue) / range, 0.0, 1.0);
		if secondaryRatio <= 0.0 {
			return;
		}
		let secondaryRect = match self.direction {
			ProgressDirection::vertical => {
				let secondaryHeight = contentSize.y * secondaryRatio;
				Rect::create(0.0, 0.0, contentSize.x, secondaryHeight)
			}
			ProgressDirection::horizontal => {
				let secondaryWidth = contentSize.x * secondaryRatio;
				Rect::create(contentSize.x - secondaryWidth, 0.0, secondaryWidth, contentSize.y)
			}
		};
		let secondaryFillColor = self.secondaryFillColor;
		self.fillRect(graphic, secondaryRect, secondaryFillColor);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 값은 범위로 잘리고, 비율은 범위 안의 자리다.
	//==============================================================================
	#[test]
	fn valueClampAndRatio() {
		let mut progressView = UIProgressView::new();
		progressView.setRange(10.0, 20.0);
		assert_eq!(progressView.getValue(), 10.0);
		progressView.setValue(15.0);
		assert_eq!(progressView.getRatio(), 0.5);
		progressView.setValue(99.0);
		assert_eq!(progressView.getValue(), 20.0);
		progressView.setSecondaryValue(Some(-5.0));
		assert_eq!(progressView.getSecondaryValue(), Some(10.0));
		progressView.setRange(0.0, 0.0);
		assert_eq!(progressView.getRatio(), 0.0);
		progressView.setTrackColor(Color::black());
		assert_eq!(progressView.getBackgroundColor(), Color::black());
	}
}
