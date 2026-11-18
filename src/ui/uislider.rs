//==============================================================================
// 슬라이더. (원본 ui/uislider.js 의 UISlider)
// - 끌거나 눌러서 값을 고른다. 트랙 + 채움 + 손잡이(원)를 직접 그린다. 값은 범위로 자르기만 한다. (원본에도 눈금 맞추기는 없다)
// - 손잡이 노드(setThumbNode)를 정하면 원을 그리지 않고 그 노드를 손잡이 자리로 옮긴다.
// - 원본과 다른 것:
//   - 원본은 draw 안에서 손잡이 노드를 옮긴다. 러스트 판은 그리는 동안 트리를 바꿀 수 없어 tick 에서 옮긴다. (손잡이 노드는 호스트의 자식으로 둔다)
//   - 입력 좌표는 원본처럼 호스트 왼쪽 위 모서리 기준이되, 노드 행렬의 역으로 바꿔 크기와 회전까지 따른다. (원본은 모서리 위치만 뺀다)
//   - 값 바뀜 이벤트는 슬라이더와 문맥을 받는다. 씬 코드에서 부르는 fireValueChanged 는 다음 tick 에 이벤트를 부른다.
// - attachToTree 에서 호스트 노드를 interactable 로 켠다. nodeTree.addComponent(nodeId, UISlider::new()) 로 붙인다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uicontrol::UIControl;
use crate::ui::uiprogressview::ProgressDirection;

//==============================================================================
// 슬라이더 이벤트. (슬라이더와 문맥을 받는다)
//==============================================================================
pub type UISliderEvent = Box<dyn FnMut(&mut UISlider, &mut ComponentContext)>;

//==============================================================================
// 슬라이더.
//==============================================================================
pub struct UISlider {
	uiControl: UIControl,
	value: f32,
	minValue: f32,
	maxValue: f32,
	trackColor: Color,
	fillColor: Color,
	thumbColor: Color,
	thumbRadius: f32,
	trackThickness: f32,
	thumbNodeId: Option<NodeId>,
	cornerRadius: f32,
	direction: ProgressDirection,
	isDragging: bool,
	isInteractable: bool,
	valueChangedEvent: Option<UISliderEvent>,
	isValueChangedFirePending: bool,
}

impl UISlider {
	//==============================================================================
	// 생성. (값 0, 범위 0 ~ 1, 트랙 회색, 채움 파랑, 손잡이 흰 원 반지름 16)
	//==============================================================================
	pub fn new() -> UISlider {
		UISlider {
			uiControl: UIControl::new(),
			value: 0.0,
			minValue: 0.0,
			maxValue: 1.0,
			trackColor: Color::create(0.7, 0.7, 0.7, 1.0),
			fillColor: Color::create(0.23, 0.51, 0.96, 1.0),
			thumbColor: Color::white(),
			thumbRadius: 16.0,
			trackThickness: 0.0,
			thumbNodeId: None,
			cornerRadius: 0.0,
			direction: ProgressDirection::horizontal,
			isDragging: false,
			isInteractable: true,
			valueChangedEvent: None,
			isValueChangedFirePending: false,
		}
	}

	//==============================================================================
	// 품은 컨트롤 반환.
	//==============================================================================
	pub fn getUIControl(&self) -> &UIControl {
		&self.uiControl
	}

	//==============================================================================
	// 품은 컨트롤 반환. (변경용)
	//==============================================================================
	pub fn getUIControlMut(&mut self) -> &mut UIControl {
		&mut self.uiControl
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiControl.getContent()
	}

	//==============================================================================
	// 손잡이 중심 자리. (호스트 영역 안)
	//==============================================================================
	fn calculateThumbCenter(&self, contentSize: Vector2) -> Vector2 {
		let ratio = self.getRatio();
		match self.direction {
			ProgressDirection::vertical => {
				Vector2::create(contentSize.x * 0.5, contentSize.y * (1.0 - ratio))
			}
			ProgressDirection::horizontal => {
				Vector2::create(contentSize.x * ratio, contentSize.y * 0.5)
			}
		}
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
	// 값 바뀜 이벤트 부르기.
	//==============================================================================
	fn fireValueChangedEvent(&mut self, context: &mut ComponentContext) {
		if let Some(mut valueChangedEvent) = self.valueChangedEvent.take() {
			valueChangedEvent(self, context);
			if self.valueChangedEvent.is_none() {
				self.valueChangedEvent = Some(valueChangedEvent);
			}
		}
	}

	//==============================================================================
	// 입력 좌표로 값 갱신. (바뀌었으면 이벤트)
	//==============================================================================
	fn updateValueFromInput(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		let nodeId = context.getNodeId();
		let localPosition = match context.getNodeTree().convertViewPositionToNodePosition(nodeId, viewInputPosition) {
			Some(localPosition) => {
				localPosition
			}
			None => {
				return;
			}
		};
		let contentSize = context.getTransform().getContentSize();
		let ratio = match self.direction {
			ProgressDirection::vertical => {
				if contentSize.y > 0.0 {
					1.0 - localPosition.y / contentSize.y
				}
				else {
					0.0
				}
			}
			ProgressDirection::horizontal => {
				if contentSize.x > 0.0 {
					localPosition.x / contentSize.x
				}
				else {
					0.0
				}
			}
		};
		let clampedRatio = math::clamp(ratio, 0.0, 1.0);
		let previousValue = self.value;
		let newValue = self.minValue + (self.maxValue - self.minValue) * clampedRatio;
		self.setValue(newValue);
		if self.value != previousValue {
			self.fireValueChangedEvent(context);
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
	// 값 설정. (범위로 자른다. 이벤트는 부르지 않는다)
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
	// 값 바뀜 이벤트를 억지로 부르기. (씬 코드에서 setValue 한 뒤. 다음 tick 에 부른다)
	//==============================================================================
	pub fn fireValueChanged(&mut self) {
		self.isValueChangedFirePending = true;
	}

	//==============================================================================
	// 트랙 색 설정.
	//==============================================================================
	pub fn setTrackColor(&mut self, color: impl Into<Color>) {
		self.trackColor = color.into();
	}

	//==============================================================================
	// 트랙 색 반환.
	//==============================================================================
	pub fn getTrackColor(&self) -> Color {
		self.trackColor
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
	// 손잡이 색 설정.
	//==============================================================================
	pub fn setThumbColor(&mut self, color: impl Into<Color>) {
		self.thumbColor = color.into();
	}

	//==============================================================================
	// 손잡이 색 반환.
	//==============================================================================
	pub fn getThumbColor(&self) -> Color {
		self.thumbColor
	}

	//==============================================================================
	// 손잡이 반지름 설정.
	//==============================================================================
	pub fn setThumbRadius(&mut self, thumbRadius: f32) {
		self.thumbRadius = thumbRadius;
	}

	//==============================================================================
	// 손잡이 반지름 반환.
	//==============================================================================
	pub fn getThumbRadius(&self) -> f32 {
		self.thumbRadius
	}

	//==============================================================================
	// 트랙 두께 설정. (0 이면 호스트 영역 전체, 아니면 가운데에 그 두께로)
	//==============================================================================
	pub fn setTrackThickness(&mut self, trackThickness: f32) {
		self.trackThickness = trackThickness;
	}

	//==============================================================================
	// 트랙 두께 반환.
	//==============================================================================
	pub fn getTrackThickness(&self) -> f32 {
		self.trackThickness
	}

	//==============================================================================
	// 손잡이 노드 설정. (호스트의 자식. 정하면 원 대신 이 노드를 손잡이 자리로 옮긴다)
	//==============================================================================
	pub fn setThumbNode(&mut self, thumbNodeId: Option<NodeId>) {
		self.thumbNodeId = thumbNodeId;
	}

	//==============================================================================
	// 손잡이 노드 반환.
	//==============================================================================
	pub fn getThumbNode(&self) -> Option<NodeId> {
		self.thumbNodeId
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

	//==============================================================================
	// 값 바뀜 이벤트 설정.
	//==============================================================================
	pub fn setValueChangedEvent(&mut self, valueChangedEvent: UISliderEvent) {
		self.valueChangedEvent = Some(valueChangedEvent);
	}

	//==============================================================================
	// 사용 가능 설정. (끄면 누름을 받지 않는다)
	//==============================================================================
	pub fn setInteractable(&mut self, isInteractable: bool) {
		self.isInteractable = isInteractable;
	}

	//==============================================================================
	// 사용 가능 여부.
	//==============================================================================
	pub fn getInteractable(&self) -> bool {
		self.isInteractable
	}

	//==============================================================================
	// 끄는 중인지.
	//==============================================================================
	pub fn isDragging(&self) -> bool {
		self.isDragging
	}
}

impl Default for UISlider {
	fn default() -> UISlider {
		UISlider::new()
	}
}

impl Component for UISlider {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiControl.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiControl.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UISlider"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiControl.getUIViewMut().createContentNode(nodeTree, nodeId);
		nodeTree[nodeId].setInteractable(true);
	}

	fn isTickNeeded(&self) -> bool {
		self.thumbNodeId.is_some() || self.isValueChangedFirePending
	}

	//==============================================================================
	// 갱신. (미뤄 둔 값 바뀜 이벤트, 손잡이 노드 자리)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _timeDelta: f32) {
		if self.isValueChangedFirePending {
			self.isValueChangedFirePending = false;
			self.fireValueChangedEvent(context);
		}
		let thumbNodeId = match self.thumbNodeId {
			Some(thumbNodeId) => {
				thumbNodeId
			}
			None => {
				return;
			}
		};
		let contentSize = context.getTransform().getContentSize();
		let thumbCenter = self.calculateThumbCenter(contentSize);
		if let Some(thumbNode) = context.getNodeTreeMut().getNodeMut(thumbNodeId) {
			// 원본처럼 로컬 위치를 (중심 - 크기 절반) 에 둔다. (피봇이 왼쪽 위인 손잡이 노드면 가운데가 손잡이 자리에 온다)
			let thumbSize = thumbNode.getContentSize();
			let thumbPosition = Vector2::create(thumbCenter.x - thumbSize.x * 0.5, thumbCenter.y - thumbSize.y * 0.5);
			thumbNode.setLocalPosition(thumbPosition);
		}
	}

	//==============================================================================
	// 출력. (트랙 + 채움 + 손잡이)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		if contentSize.x <= 0.0 || contentSize.y <= 0.0 {
			return;
		}

		// 트랙. (두께를 정했으면 노드 가운데에 그 두께로 그린다)
		let trackThickness = self.trackThickness;
		let mut trackRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
		if trackThickness > 0.0 {
			trackRect = match self.direction {
				ProgressDirection::vertical => {
					Rect::create((contentSize.x - trackThickness) * 0.5, 0.0, trackThickness, contentSize.y)
				}
				ProgressDirection::horizontal => {
					Rect::create(0.0, (contentSize.y - trackThickness) * 0.5, contentSize.x, trackThickness)
				}
			};
		}
		let trackColor = self.trackColor;
		self.fillRect(graphic, trackRect, trackColor);

		// 채움.
		let ratio = self.getRatio();
		if ratio > 0.0 {
			let fillRect = match self.direction {
				ProgressDirection::vertical => {
					let fillHeight = contentSize.y * ratio;
					Rect::create(trackRect.position.x, contentSize.y - fillHeight, trackRect.size.x, fillHeight)
				}
				ProgressDirection::horizontal => {
					let fillWidth = contentSize.x * ratio;
					Rect::create(0.0, trackRect.position.y, fillWidth, trackRect.size.y)
				}
			};
			let fillColor = self.fillColor;
			self.fillRect(graphic, fillRect, fillColor);
		}

		// 손잡이 노드를 정했으면 원은 그리지 않는다. (자리는 tick 이 옮긴다)
		if self.thumbNodeId.is_some() {
			return;
		}
		let thumbCenter = self.calculateThumbCenter(contentSize);
		graphic.setFillColor(self.thumbColor);
		graphic.drawCircle(thumbCenter, self.thumbRadius);
	}

	fn touchPress(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isInteractable {
			return;
		}
		self.isDragging = true;
		self.updateValueFromInput(context, viewInputPosition);
	}

	fn touchMove(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isDragging {
			return;
		}
		self.updateValueFromInput(context, viewInputPosition);
	}

	fn touchRelease(&mut self, context: &mut ComponentContext, viewInputPosition: Vector2) {
		if !self.isDragging {
			return;
		}
		self.updateValueFromInput(context, viewInputPosition);
		self.isDragging = false;
	}

	fn touchCancel(&mut self, _context: &mut ComponentContext, _viewInputPosition: Vector2) {
		self.isDragging = false;
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;
	use std::cell::Cell;
	use std::rc::Rc;

	//==============================================================================
	// 값은 범위로 잘린다. 범위를 바꾸면 지금 값도 다시 잘린다.
	//==============================================================================
	#[test]
	fn valueClamp() {
		let mut slider = UISlider::new();
		slider.setValue(2.0);
		assert_eq!(slider.getValue(), 1.0);
		slider.setValue(-1.0);
		assert_eq!(slider.getValue(), 0.0);
		slider.setRange(10.0, 20.0);
		assert_eq!(slider.getValue(), 10.0);
		slider.setValue(17.5);
		assert_eq!(slider.getRatio(), 0.75);
		slider.setRange(0.0, 5.0);
		assert_eq!(slider.getValue(), 5.0);
	}

	//==============================================================================
	// 누르고 끌면 그 자리 비율로 값이 바뀌고 이벤트가 난다. 영역 밖으로 끌면 끝 값에 멈춘다.
	//==============================================================================
	#[test]
	fn dragUpdatesValue() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut nodeTree = NodeTree::new();
		let rootId = nodeTree.getRootId();
		let sliderId = nodeTree.createChild(rootId, "slider");
		let sliderNode = &mut nodeTree[sliderId];
		sliderNode.setPivot(Pivot::topLeft);
		sliderNode.setLocalPosition(Vector2::create(100.0, 50.0));
		sliderNode.setContentSize(Vector2::create(200.0, 20.0));
		let changedCount = Rc::new(Cell::new(0));
		{
			let slider = nodeTree.addComponent(sliderId, UISlider::new()).expect("slider");
			slider.setRange(0.0, 100.0);
			let changedCountForEvent = changedCount.clone();
			slider.setValueChangedEvent(Box::new(move |_slider: &mut UISlider, _context: &mut ComponentContext| {
				changedCountForEvent.set(changedCountForEvent.get() + 1);
			}));
		}
		assert!(nodeTree[sliderId].isInteractable());

		nodeTree.dispatchTouch(sliderId, &engine, TouchKind::press, Vector2::create(150.0, 60.0));
		assert_eq!(nodeTree[sliderId].getComponent::<UISlider>().expect("slider").getValue(), 25.0);
		assert_eq!(changedCount.get(), 1);
		nodeTree.dispatchTouch(sliderId, &engine, TouchKind::moved, Vector2::create(500.0, 60.0));
		assert_eq!(nodeTree[sliderId].getComponent::<UISlider>().expect("slider").getValue(), 100.0);
		assert_eq!(changedCount.get(), 2);
		// 같은 값이면 이벤트가 나지 않는다.
		nodeTree.dispatchTouch(sliderId, &engine, TouchKind::moved, Vector2::create(600.0, 60.0));
		assert_eq!(changedCount.get(), 2);
		nodeTree.dispatchTouch(sliderId, &engine, TouchKind::release, Vector2::create(100.0, 60.0));
		assert_eq!(nodeTree[sliderId].getComponent::<UISlider>().expect("slider").getValue(), 0.0);
		assert_eq!(changedCount.get(), 3);
		assert!(!nodeTree[sliderId].getComponent::<UISlider>().expect("slider").isDragging());

		// 손잡이 노드는 tick 에서 손잡이 자리로 옮겨진다.
		let thumbId = nodeTree.createChild(sliderId, "thumb");
		nodeTree[thumbId].setPivot(Pivot::topLeft);
		nodeTree[thumbId].setContentSize(Vector2::create(10.0, 10.0));
		{
			let slider = nodeTree[sliderId].getComponentMut::<UISlider>().expect("slider");
			slider.setThumbNode(Some(thumbId));
			slider.setValue(50.0);
			slider.fireValueChanged();
		}
		assert_eq!(changedCount.get(), 3);
		nodeTree.tick(&engine, 0.016);
		assert_eq!(nodeTree[thumbId].getLocalPosition(), Vector2::create(95.0, 5.0));
		assert_eq!(changedCount.get(), 4);
	}
}
