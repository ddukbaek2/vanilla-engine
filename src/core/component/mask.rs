//==============================================================================
//! 마스크 컴포넌트. (붙은 노드의 content_size 영역으로 자식 출력을 자른다)
//! - 실제 자르기는 NodeTree::draw_node 가 자식을 그리기 전후로 begin_clip / end_clip 을 부른다.
//==============================================================================
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
/// 마스크 컴포넌트. (붙은 노드의 content_size 영역 밖으로 나가는 자식 노드 출력을 자른다)
/// - 노드 자신의 컴포넌트는 자르지 않는다. 컴포넌트를 다 그린 뒤 자식을 그리는 동안만 자른다.
/// - 둥근 크기를 주면 둥근 사각형으로 자른다. 배경 Paint 와 같은 둥근 크기를 주면 배경 모양 그대로 잘린다.
///
/// # 예제
/// 둥근 창 안으로 자식 자르기
/// ```rust
/// use vanilla::{Mask, NodeTree, Paint, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let window_id = node_tree.create_child(root_id, "Window");
/// node_tree[window_id].set_content_size(Vector2::create(200.0, 200.0));
/// let paint = node_tree.add_component(window_id, Paint::create_with_color("#223344")).unwrap();
/// paint.set_round_size(24.0);
/// let mask = node_tree.add_component(window_id, Mask::new()).unwrap();
/// mask.set_round_size(24.0);
///
/// let child_id = node_tree.create_child(window_id, "Child");
/// node_tree[child_id].set_content_size(Vector2::create(160.0, 160.0));
/// node_tree[child_id].set_local_position(Vector2::create(120.0, 120.0));
/// node_tree.add_component(child_id, Paint::create_with_color("#ff6633"));
///
/// let mask = node_tree[window_id].get_component::<Mask>().unwrap();
/// assert_eq!(mask.get_round_size(), 24.0);
/// ```
//==============================================================================
pub struct Mask {
	component_base: ComponentBase,
	round_size: f64,
}

#[crate::reflect]
impl Mask {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 둥근 크기가 0 인(곧은 사각형으로 자르는) 새 마스크.
	//==============================================================================
	pub fn new() -> Mask {
		Mask {
			component_base: ComponentBase::new(),
			round_size: 0.0,
		}
	}

	//==============================================================================
	/// 자르기 시작.
	/// - 노드 content 좌표의 (0, 0, content_size 너비, 높이) 사각형을 Graphic 의 자르기 영역으로 건다. (스텐실이라 회전, 배율이 걸려도 정확히 잘린다)
	/// - NodeTree 가 마스크 붙은 노드의 자식을 그리기 바로 전에 부른다. end_clip 과 짝으로 부르고, 그 사이에 그린 것만 잘린다.
	///
	/// # 인자
	/// - `transform`: 자를 영역을 정하는 노드 트랜스폼. (content_size 를 읽는다)
	/// - `round_size`: 귀퉁이 둥근 크기. (노드 좌표, 0 이면 곧은 사각형)
	/// - `graphic`: 자르기 영역을 걸 그래픽.
	///
	/// # 예제
	/// 제 그림을 노드 영역으로 잘라 그리기
	/// ```rust,no_run
	/// use vanilla::{Color, Graphic, Mask, NodeTransform, Rect};
	///
	/// fn draw_clipped(transform: &NodeTransform, graphic: &mut Graphic) {
	/// 	Mask::begin_clip(transform, 8.0, graphic);
	/// 	graphic.set_fill_color(Color::white());
	/// 	graphic.draw_rect(Rect::create(-20.0, -20.0, 400.0, 400.0));
	/// 	Mask::end_clip(graphic);
	/// }
	/// ```
	//==============================================================================
	pub fn begin_clip(transform: &NodeTransform, round_size: f64, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let clip_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		graphic.begin_clip_rect(clip_rect, round_size);
	}

	//==============================================================================
	/// 자르기 끝.
	/// - begin_clip 으로 건 자르기 영역을 푼다. (NodeTree 가 자식을 다 그린 뒤 부른다)
	///
	/// # 인자
	/// - `graphic`: begin_clip 에 넘긴 그래픽.
	//==============================================================================
	pub fn end_clip(graphic: &mut Graphic) {
		graphic.end_clip_rect();
	}

	//==============================================================================
	/// 라운드 크기 설정. (Paint 의 round_size 와 같은 값이면 배경과 정확히 맞는다)
	///
	/// # 인자
	/// - `round_size`: 자르는 사각형의 귀퉁이 둥근 크기. (노드 좌표, 0 이면 곧은 사각형, 처음 0)
	//==============================================================================
	pub fn set_round_size(&mut self, round_size: f64) {
		self.round_size = round_size;
	}

	//==============================================================================
	/// 라운드 크기 반환.
	///
	/// # 돌려줌
	/// 자르는 사각형의 귀퉁이 둥근 크기. (노드 좌표)
	//==============================================================================
	pub fn get_round_size(&self) -> f64 {
		self.round_size
	}
}

impl Default for Mask {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Mask {
		Mask::new()
	}
}

#[crate::reflect]
impl Component for Mask {
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
	/// 컴포넌트 이름. ("Mask")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Mask"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (자르기만 하므로 false)
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
	/// 자식 출력을 자를 둥근 크기. (늘 Some(round_size), NodeTree 가 보고 begin_clip 을 부른다)
	//==============================================================================
	#[reflect(skip)]
	fn get_clip_round_size(&self) -> Option<f64> {
		Some(self.round_size)
	}
}
