//==============================================================================
// 테만 그리는 컴포넌트. (vanilla.js core/component/outline.js 의 Outline)
// - Paint 는 속을 채우기만 하므로, 비치는 판 위에 테를 따로 얹을 때 쓴다. (채운 판 두 장을 겹치면 비치는 속에 바깥 색이 섞인다)
// - 둥근 네모 테는 노드의 내용 크기 그대로, 동그라미 테는 그 가운데에 짧은 변의 반을 반지름으로 그린다.
// - 선은 내용 크기 안쪽에 들어온다. (선의 가운데가 굵기의 반만큼 안쪽)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

pub struct Outline {
	component_base: ComponentBase,
	color: Color,
	line_width: f32,
	round_size: f32,
	is_circle: bool,
}

#[crate::reflect]
impl Outline {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Outline {
		Outline {
			component_base: ComponentBase::new(),
			color: Color::white(),
			line_width: 1.0,
			round_size: 0.0,
			is_circle: false,
		}
	}

	//==============================================================================
	// 출력.
	//==============================================================================
	pub fn draw_outline(&self, transform: &NodeTransform, graphic: &mut Graphic) {
		let line_width = self.line_width;
		if line_width <= 0.0 {
			return;
		}
		let content_size = transform.get_content_size();
		let half_line_width = line_width * 0.5;
		graphic.set_stroke_color(self.color);
		if self.is_circle {
			let radius = content_size.x.min(content_size.y) * 0.5 - half_line_width;
			if radius <= 0.0 {
				return;
			}
			let center = Vector2::create(content_size.x * 0.5, content_size.y * 0.5);
			graphic.draw_stroke_circle(center, radius, line_width);
			return;
		}
		let width = content_size.x - line_width;
		let height = content_size.y - line_width;
		if width <= 0.0 || height <= 0.0 {
			return;
		}
		let rect = Rect::create(half_line_width, half_line_width, width, height);
		let round_size = (self.round_size - half_line_width).max(0.0);
		graphic.draw_stroke_round_rect(rect, round_size, line_width);
	}

	//==============================================================================
	// 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	// 색 반환.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	// 굵기 설정. (0 이면 그리지 않는다)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f32) {
		self.line_width = line_width;
	}

	//==============================================================================
	// 굵기 반환.
	//==============================================================================
	pub fn get_line_width(&self) -> f32 {
		self.line_width
	}

	//==============================================================================
	// 귀퉁이 둥근 정도 설정. (바깥 꼴 기준)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f32) {
		self.round_size = round_size;
	}

	//==============================================================================
	// 귀퉁이 둥근 정도 반환.
	//==============================================================================
	pub fn get_round_size(&self) -> f32 {
		self.round_size
	}

	//==============================================================================
	// 동그라미 테로 그릴지 설정.
	//==============================================================================
	pub fn set_circle(&mut self, is_circle: bool) {
		self.is_circle = is_circle;
	}

	//==============================================================================
	// 동그라미 테인지 반환.
	//==============================================================================
	pub fn is_circle(&self) -> bool {
		self.is_circle
	}
}

impl Default for Outline {
	fn default() -> Outline {
		Outline::new()
	}
}

#[crate::reflect]
impl Component for Outline {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"Outline"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_outline(transform, graphic);
	}
}
