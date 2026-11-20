//==============================================================================
// 마스크 컴포넌트. (붙은 노드의 content_size 영역으로 자식 출력을 자른다)
// - 실제 자르기는 WorldNode::draw 가 자식을 그리기 전후로 begin_clip / end_clip 을 부른다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

pub struct Mask {
	component_base: ComponentBase,
	round_size: f32,
}

impl Mask {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Mask {
		Mask {
			component_base: ComponentBase::new(),
			round_size: 0.0,
		}
	}

	//==============================================================================
	// 자르기 시작.
	//==============================================================================
	pub fn begin_clip(transform: &NodeTransform, round_size: f32, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let clip_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		graphic.begin_clip_rect(clip_rect, round_size);
	}

	//==============================================================================
	// 자르기 끝.
	//==============================================================================
	pub fn end_clip(graphic: &mut Graphic) {
		graphic.end_clip_rect();
	}

	//==============================================================================
	// 라운드 크기 설정. (Paint 의 round_size 와 같은 값이면 배경과 정확히 맞는다)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f32) {
		self.round_size = round_size;
	}

	//==============================================================================
	// 라운드 크기 반환.
	//==============================================================================
	pub fn get_round_size(&self) -> f32 {
		self.round_size
	}
}

impl Default for Mask {
	fn default() -> Mask {
		Mask::new()
	}
}

impl Component for Mask {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Mask"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn get_clip_round_size(&self) -> Option<f32> {
		Some(self.round_size)
	}
}
