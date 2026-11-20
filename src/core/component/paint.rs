//==============================================================================
// 색 채우기 컴포넌트. (노드 content_size 영역을 단색 / 둥근 사각형으로 칠한다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

pub struct Paint {
	component_base: ComponentBase,
	color: Color,
	round_size: f32,
}

impl Paint {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Paint {
		Paint {
			component_base: ComponentBase::new(),
			color: Color::white(),
			round_size: 0.0,
		}
	}

	//==============================================================================
	// 색과 함께 생성.
	//==============================================================================
	pub fn create_with_color(color: impl Into<Color>) -> Paint {
		let mut paint = Paint::new();
		paint.set_color(color);
		paint
	}

	//==============================================================================
	// 출력. (Sprite 가 이미지 없을 때도 이것을 쓴다)
	//==============================================================================
	pub fn draw_paint(&self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let color = self.get_color();
		let rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);

		graphic.set_fill_color(color);
		if self.round_size > 0.0 {
			graphic.draw_round_rect(rect, self.round_size);
		}
		else {
			graphic.draw_rect(rect);
		}
	}

	//==============================================================================
	// 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	// 라운드 크기 설정.
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

impl Default for Paint {
	fn default() -> Paint {
		Paint::new()
	}
}

impl Component for Paint {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Paint"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_paint(transform, graphic);
	}
}
