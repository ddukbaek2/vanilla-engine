//==============================================================================
//! 테만 그리는 컴포넌트. (vanilla.js core/component/outline.js 의 Outline)
//! - Paint 는 속을 채우기만 하므로, 비치는 판 위에 테를 따로 얹을 때 쓴다. (채운 판 두 장을 겹치면 비치는 속에 바깥 색이 섞인다)
//! - 둥근 네모 테는 노드의 내용 크기 그대로, 동그라미 테는 그 가운데에 짧은 변의 반을 반지름으로 그린다.
//! - 선은 내용 크기 안쪽에 들어온다. (선의 가운데가 굵기의 반만큼 안쪽)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
/// 테만 그리는 컴포넌트. (속은 비우고 노드 content_size 의 테두리만 긋는다)
/// - 처음은 흰색, 굵기 1, 둥근 크기 0, 네모 테.
/// - 선은 내용 크기 안쪽에 그어 바깥 꼴이 내용 크기와 같다. 둥근 크기도 바깥 꼴 기준이다.
///
/// # 예제
/// 비치는 판 위에 둥근 테 얹기
/// ```rust
/// use vanilla::{Color, NodeTree, Outline, Paint, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let card_id = node_tree.create_child(root_id, "Card");
/// node_tree[card_id].set_content_size(Vector2::create(200.0, 120.0));
/// node_tree.add_component(card_id, Paint::create_with_color(Color::black().with_alpha(0.4)));
/// let outline = node_tree.add_component(card_id, Outline::new()).unwrap();
/// outline.set_color("#ffcc00");
/// outline.set_line_width(3.0);
/// outline.set_round_size(12.0);
///
/// let outline = node_tree[card_id].get_component::<Outline>().unwrap();
/// assert_eq!(outline.get_line_width(), 3.0);
/// assert!(!outline.is_circle());
/// ```
//==============================================================================
pub struct Outline {
	component_base: ComponentBase,
	color: Color,
	line_width: f64,
	round_size: f64,
	is_circle: bool,
}

#[crate::reflect]
impl Outline {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 흰색, 굵기 1, 둥근 크기 0 인 네모 테 컴포넌트.
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
	/// 출력.
	/// - 굵기가 0 이하이거나, 굵기를 빼고 남는 너비, 높이(동그라미면 반지름)가 0 이하면 그리지 않는다.
	/// - Graphic 의 선 색이 이 색으로 바뀐다.
	///
	/// # 인자
	/// - `transform`: 테를 그릴 노드 트랜스폼. (content_size 를 읽는다)
	/// - `graphic`: 그릴 그래픽. (노드 트랜스폼이 걸린 상태)
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
	/// 색 설정. (Color 또는 "#rrggbb" 같은 문자열)
	///
	/// # 인자
	/// - `color`: 테 색. (알파가 불투명도, 처음 흰색)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.color = color.into();
	}

	//==============================================================================
	/// 색 반환.
	///
	/// # 돌려줌
	/// 테 색.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.color
	}

	//==============================================================================
	/// 굵기 설정. (0 이면 그리지 않는다)
	///
	/// # 인자
	/// - `line_width`: 선 굵기. (노드 좌표, 0 이하면 그리지 않는다, 처음 1)
	//==============================================================================
	pub fn set_line_width(&mut self, line_width: f64) {
		self.line_width = line_width;
	}

	//==============================================================================
	/// 굵기 반환.
	///
	/// # 돌려줌
	/// 선 굵기. (노드 좌표)
	//==============================================================================
	pub fn get_line_width(&self) -> f64 {
		self.line_width
	}

	//==============================================================================
	/// 귀퉁이 둥근 정도 설정. (바깥 꼴 기준)
	///
	/// # 인자
	/// - `round_size`: 바깥 꼴의 귀퉁이 둥근 크기. (노드 좌표, 처음 0) 선의 가운데는 굵기의 반만큼 덜 둥글게 긋는다. 동그라미 테에는 쓰지 않는다.
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f64) {
		self.round_size = round_size;
	}

	//==============================================================================
	/// 귀퉁이 둥근 정도 반환.
	///
	/// # 돌려줌
	/// 바깥 꼴의 귀퉁이 둥근 크기. (노드 좌표)
	//==============================================================================
	pub fn get_round_size(&self) -> f64 {
		self.round_size
	}

	//==============================================================================
	/// 동그라미 테로 그릴지 설정.
	///
	/// # 인자
	/// - `is_circle`: true 면 내용 크기 가운데에 짧은 변의 반을 바깥 반지름으로 하는 동그라미 테, false 면 네모 테. (처음 false)
	//==============================================================================
	pub fn set_circle(&mut self, is_circle: bool) {
		self.is_circle = is_circle;
	}

	//==============================================================================
	/// 동그라미 테인지 반환.
	///
	/// # 돌려줌
	/// 동그라미 테면 true, 네모 테면 false.
	//==============================================================================
	pub fn is_circle(&self) -> bool {
		self.is_circle
	}
}

impl Default for Outline {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Outline {
		Outline::new()
	}
}

#[crate::reflect]
impl Component for Outline {
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
	/// 컴포넌트 이름. ("Outline")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Outline"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (그리기만 하므로 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (선이 안쪽에 들어오므로 true)
	//==============================================================================
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (draw_outline)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_outline(transform, graphic);
	}
}
