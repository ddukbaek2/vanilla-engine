//==============================================================================
//! 색 채우기 컴포넌트. (노드 content_size 영역을 단색 / 둥근 사각형으로 칠한다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
/// 색 채우기 컴포넌트. (노드 content_size 영역을 한 색으로 칠한다)
/// - 둥근 크기가 0 보다 크면 둥근 사각형, 아니면 곧은 사각형으로 칠한다. (처음 흰색, 둥근 크기 0)
/// - 그리기만 하므로 tick 을 받지 않는다. Sprite 는 그림이 없을 때 이것으로 칠한다.
///
/// # 예제
/// 둥근 파란 판
/// ```rust
/// use vanilla::{Color, NodeTree, Paint, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let panel_id = node_tree.create_child(root_id, "Panel");
/// node_tree[panel_id].set_content_size(Vector2::create(240.0, 120.0));
/// let paint = node_tree.add_component(panel_id, Paint::new()).unwrap();
/// paint.set_color("#3366cc");
/// paint.set_round_size(16.0);
///
/// let paint = node_tree[panel_id].get_component::<Paint>().unwrap();
/// assert_eq!(paint.get_color(), Color::create_from_hex("#3366cc"));
/// assert_eq!(paint.get_round_size(), 16.0);
/// ```
//==============================================================================
pub struct Paint {
	component_base: ComponentBase,
	color: Color,
	round_size: f32,
}

#[crate::reflect]
impl Paint {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 흰색, 둥근 크기 0 인 새 컴포넌트.
	//==============================================================================
	pub fn new() -> Paint {
		Paint {
			component_base: ComponentBase::new(),
			color: Color::white(),
			round_size: 0.0,
		}
	}

	//==============================================================================
	/// 색과 함께 생성.
	///
	/// # 인자
	/// - `color`: 칠할 색. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 돌려줌
	/// 그 색으로 칠하는 새 컴포넌트. (둥근 크기 0)
	///
	/// # 예제
	/// 색 글로 만들어 노드에 붙이기
	/// ```rust
	/// use vanilla::{Color, NodeTree, Paint};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "Background");
	/// let paint = node_tree.add_component(node_id, Paint::create_with_color("#ff0000")).unwrap();
	/// assert_eq!(paint.get_color(), Color::create(1.0, 0.0, 0.0, 1.0));
	///
	/// let half_black = Paint::create_with_color(Color::black().with_alpha(0.5));
	/// assert_eq!(half_black.get_color().alpha, 0.5);
	/// ```
	//==============================================================================
	pub fn create_with_color(color: impl Into<Color>) -> Paint {
		let mut paint = Paint::new();
		paint.set_color(color);
		paint
	}

	//==============================================================================
	/// 출력. (Sprite 가 이미지 없을 때도 이것을 쓴다)
	/// - 노드 content 좌표의 (0, 0, content_size 너비, 높이) 사각형을 지금 색으로 채운다. Graphic 의 채우기 색이 이 색으로 바뀐다.
	///
	/// # 인자
	/// - `transform`: 칠할 노드 트랜스폼. (content_size 를 읽는다)
	/// - `graphic`: 그릴 그래픽. (노드 트랜스폼이 걸린 상태)
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
	/// 색상 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `color`: 칠할 색. (알파가 불투명도)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	/// 색상 반환.
	///
	/// # 돌려줌
	/// 칠하는 색.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 라운드 크기 설정.
	///
	/// # 인자
	/// - `round_size`: 귀퉁이 둥근 크기. (노드 좌표, 0 이하면 곧은 사각형)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f32) {
		self.round_size = round_size;
	}

	//==============================================================================
	/// 라운드 크기 반환.
	///
	/// # 돌려줌
	/// 귀퉁이 둥근 크기. (노드 좌표)
	//==============================================================================
	pub fn get_round_size(&self) -> f32 {
		self.round_size
	}
}

impl Default for Paint {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Paint {
		Paint::new()
	}
}

#[crate::reflect]
impl Component for Paint {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름. ("Paint")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Paint"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (그리기만 하므로 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (true)
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (draw_paint)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_paint(transform, graphic);
	}
}
