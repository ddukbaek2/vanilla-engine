//==============================================================================
// 마스크 컴포넌트. (붙은 노드의 contentSize 영역으로 자식 출력을 자른다)
// - 실제 자르기는 WorldNode::draw 가 자식을 그리기 전후로 beginClip / endClip 을 부른다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

pub struct Mask {
	componentBase: ComponentBase,
	roundSize: f32,
}

impl Mask {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Mask {
		Mask {
			componentBase: ComponentBase::new(),
			roundSize: 0.0,
		}
	}

	//==============================================================================
	// 자르기 시작.
	//==============================================================================
	pub fn beginClip(transform: &NodeTransform, roundSize: f32, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		let clipRect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);
		graphic.beginClipRect(clipRect, roundSize);
	}

	//==============================================================================
	// 자르기 끝.
	//==============================================================================
	pub fn endClip(graphic: &mut Graphic) {
		graphic.endClipRect();
	}

	//==============================================================================
	// 라운드 크기 설정. (Paint 의 roundSize 와 같은 값이면 배경과 정확히 맞는다)
	//==============================================================================
	pub fn setRoundSize(&mut self, roundSize: f32) {
		self.roundSize = roundSize;
	}

	//==============================================================================
	// 라운드 크기 반환.
	//==============================================================================
	pub fn getRoundSize(&self) -> f32 {
		self.roundSize
	}
}

impl Default for Mask {
	fn default() -> Mask {
		Mask::new()
	}
}

impl Component for Mask {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"Mask"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn getClipRoundSize(&self) -> Option<f32> {
		Some(self.roundSize)
	}
}
