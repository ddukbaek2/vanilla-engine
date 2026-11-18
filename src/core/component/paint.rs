//==============================================================================
// 색 채우기 컴포넌트. (노드 contentSize 영역을 단색 / 둥근 사각형으로 칠한다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

pub struct Paint {
	componentBase: ComponentBase,
	color: Color,
	roundSize: f32,
}

impl Paint {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Paint {
		Paint {
			componentBase: ComponentBase::new(),
			color: Color::white(),
			roundSize: 0.0,
		}
	}

	//==============================================================================
	// 색과 함께 생성.
	//==============================================================================
	pub fn createWithColor(color: impl Into<Color>) -> Paint {
		let mut paint = Paint::new();
		paint.setColor(color);
		paint
	}

	//==============================================================================
	// 출력. (Sprite 가 이미지 없을 때도 이것을 쓴다)
	//==============================================================================
	pub fn drawPaint(&self, transform: &NodeTransform, graphic: &mut Graphic) {
		let contentSize = transform.getContentSize();
		let color = self.getColor();
		let rect = Rect::create(0.0, 0.0, contentSize.x, contentSize.y);

		graphic.setFillColor(color);
		if self.roundSize > 0.0 {
			graphic.drawRoundRect(rect, self.roundSize);
		}
		else {
			graphic.drawRect(rect);
		}
	}

	//==============================================================================
	// 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	//==============================================================================
	pub fn setColor(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn getColor(&self) -> Color {
		self.color
	}

	//==============================================================================
	// 라운드 크기 설정.
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

impl Default for Paint {
	fn default() -> Paint {
		Paint::new()
	}
}

impl Component for Paint {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"Paint"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.drawPaint(transform, graphic);
	}
}
