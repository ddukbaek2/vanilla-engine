//==============================================================================
//! 스크롤 뷰. (원본 ui/uiscrollview.js 의 UIScrollView, ScrollMode)
//! - 호스트 content_size 를 보이는 영역으로 쓰고, content 노드를 끌거나 휠을 굴려 움직인다. 스크롤 오프셋 = content 노드의 로컬 위치.
//! - 원본처럼 호스트에 Mask 를 붙여(require) content 를 자르고, 호스트를 interactable 로 켠다. node_tree.add_component(node_id, UIScrollView::new()) 로 붙인다.
//! - 스크롤바(UIScrollBar 가 붙은 노드)는 원본 코드처럼 호스트의 자식(content 의 형제)으로 만든다.
//! - 휠: 원본 TouchRaycaster.touchWheel → UIScrollView.wheel 길. 엔진이 터치 아래 노드부터 부모 쪽으로 wheel 을 묻고, 이 컴포넌트가 받아 쓴다.
//! - 원본과 다른 것:
//!   - 원본은 set_scroll_offset / set_scroll_content_size / setShows...ScrollBar 에서 바로 노드를 바꾼다. 러스트 판의 씬 코드는 컴포넌트만 쥐고 있어
//!     값을 정해 두고 다음 tick 에 content 노드와 스크롤바 노드에 반영한다. 같은 프레임에 바로 옮기려면 UIScrollView::set_scroll_offset_in_tree 를 쓴다.
//!   - 그래서 get_vertical_scroll_bar / get_horizontal_scroll_bar 는 보이기로 정한 뒤 첫 tick 이 지나야 노드 핸들을 돌려준다.
//!     원본처럼 그 자리에서 스크롤바 노드를 만들고 켜려면 UIScrollView::set_shows_vertical_scroll_bar_in_tree(가로는 horizontal)를 쓴다. (만든 노드 핸들을 돌려준다)
//!   - 보이는 영역 크기(호스트 content_size)는 붙을 때와 tick / 터치 / 휠 때 읽어 둔다. 씬 코드에서 정한 오프셋은 다음 tick 에 새 크기로 다시 자른다.
//!   - 드래그를 위 스크롤 뷰로 넘기는 손짓(원본 TouchRecognizer)은 ui/touchrecognizer.rs 에 따로 있다. 씬이 골라 쓴다.
//! - 원본 그대로인 버릇: 콘텐트가 영역 안에 들어와 스스로 꺼진(자동 숨김) 스크롤바 노드는 tick 이 돌지 않아 저절로 다시 켜지지 않는다.
//!   setShows...ScrollBar(true) 를 다시 부르면 켜진다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiscrollbar::{ScrollBarAxis, UIScrollBar};
use crate::ui::uiview::UIView;

/// 스크롤바 기본 두께 / 여백. (UIKit 표시기 꼴)
const SCROLLBAR_THICKNESS: f32 = 12.0;
const SCROLLBAR_MARGIN: f32 = 6.0;
const WHEEL_SCROLL_SCALE: f32 = 1.0;

/// 탄성 모드 상수. (원본과 같다)
const ELASTIC_RESISTANCE: f32 = 0.5;
const SPRING_CONSTANT: f32 = 1200.0;
const DAMPING_COEFFICIENT: f32 = 30.0;
const FRICTION_COEFFICIENT: f32 = 5.0;

//==============================================================================
/// 스크롤 모드.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollMode {
	/// 경계를 넘지 않는 기본 모드.
	Clamp,
	/// 경계를 넘으면 저항이 생기고, 놓으면 스프링처럼 튕겨 돌아온다.
	Elastic,
}

// 스크롤 방식의 반사 값. (원본 ScrollMode 글)
crate::core::reflection::impl_reflect_names!(ScrollMode, [
	(ScrollMode::Clamp, "clamp"),
	(ScrollMode::Elastic, "elastic"),
]);

//==============================================================================
/// 스크롤 뷰.
/// - 호스트 content_size 가 보이는 영역이고, content 노드에 붙인 자식들이 오프셋만큼 움직인다. 오프셋은 0 에서 (보이는 크기 - 콘텐트 크기) 사이다.
///
/// # 예제
/// 세로로 긴 목록을 담고 끝까지 내리기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, NodeTree, Pivot, UIScrollView, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let scroll_id = node_tree.create_child(root_id, "scroll");
/// node_tree[scroll_id].set_pivot(Pivot::TOP_LEFT);
/// node_tree[scroll_id].set_content_size(Vector2::create(200.0, 100.0));
/// let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
/// scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
/// let content_id = scroll_view.get_content().expect("content");
/// for index in 0..10 {
/// 	let item_id = node_tree.create_child(content_id, "item");
/// 	node_tree[item_id].set_pivot(Pivot::TOP_LEFT);
/// 	node_tree[item_id].set_rect(0.0, index as f32 * 40.0, 200.0, 40.0);
/// }
///
/// let scroll_view = node_tree[scroll_id].get_component_mut::<UIScrollView>().expect("scroll view");
/// scroll_view.set_scroll_offset(Vector2::create(0.0, -1000.0));
/// assert_eq!(scroll_view.get_scroll_offset(), Vector2::create(0.0, -300.0));
/// node_tree.tick(&engine, 0.016);
/// assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -300.0));
/// assert_eq!(node_tree[content_id].get_content_size(), Vector2::create(200.0, 400.0));
/// ```
//==============================================================================
pub struct UIScrollView {
	ui_view: UIView,
	scroll_content_size: Vector2,
	scroll_mode: ScrollMode,
	scroll_velocity: Vector2,
	previous_view_input_position: Vector2,
	current_view_input_position: Vector2,
	horizontal_enabled: bool,
	vertical_enabled: bool,
	scroll_offset: Vector2,
	is_dragging: bool,
	drag_start_view_position: Vector2,
	drag_start_offset: Vector2,
	drag_sensitivity: f32,
	vertical_scroll_bar_id: Option<NodeId>,
	horizontal_scroll_bar_id: Option<NodeId>,
	shows_vertical_scroll_bar: bool,
	shows_horizontal_scroll_bar: bool,
	pending_vertical_scroll_bar_active: Option<bool>,
	pending_horizontal_scroll_bar_active: Option<bool>,
	wheel_scroll_scale: f32,
	passes_cross_axis_drag: bool,
	viewport_size: Vector2,
	is_offset_clamp_pending: bool,
	is_content_position_sync_pending: bool,
	is_content_size_sync_pending: bool,
}

#[crate::reflect(base = ui_view)]
impl UIScrollView {
	//==============================================================================
	/// 생성.
	/// - 노드에 붙일 때는 node_tree.add_component 를 쓴다. (붙을 때 content 노드와 Mask 를 만들고 interactable 을 켠다)
	///
	/// # 돌려줌
	/// 가로 세로 모두 켜진, 오프셋 0, 클램프 모드의 새 스크롤 뷰. (스크롤바 없음, 배경 투명)
	//==============================================================================
	pub fn new() -> UIScrollView {
		UIScrollView {
			ui_view: UIView::new(),
			scroll_content_size: Vector2::zero(),
			scroll_mode: ScrollMode::Clamp,
			scroll_velocity: Vector2::zero(),
			previous_view_input_position: Vector2::zero(),
			current_view_input_position: Vector2::zero(),
			horizontal_enabled: true,
			vertical_enabled: true,
			scroll_offset: Vector2::zero(),
			is_dragging: false,
			drag_start_view_position: Vector2::zero(),
			drag_start_offset: Vector2::zero(),
			drag_sensitivity: 1.0,
			vertical_scroll_bar_id: None,
			horizontal_scroll_bar_id: None,
			shows_vertical_scroll_bar: false,
			shows_horizontal_scroll_bar: false,
			pending_vertical_scroll_bar_active: None,
			pending_horizontal_scroll_bar_active: None,
			wheel_scroll_scale: WHEEL_SCROLL_SCALE,
			passes_cross_axis_drag: false,
			viewport_size: Vector2::zero(),
			is_offset_clamp_pending: false,
			is_content_position_sync_pending: false,
			is_content_size_sync_pending: false,
		}
	}

	//==============================================================================
	/// 품은 뷰 반환.
	/// # 돌려줌
	/// 배경색과 content 노드를 든 품은 뷰.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	/// 품은 뷰 반환. (변경용)
	/// # 돌려줌
	/// 품은 뷰의 가변 참조.
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	/// 컨텐트 노드 핸들. (스크롤할 자식 노드를 여기에 붙인다)
	/// # 돌려줌
	/// content 노드. 트리에 붙기(node_tree.add_component) 전에는 None.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	/// 배경색 설정.
	/// # 인자
	/// - `background_color`: 보이는 영역의 바탕색. (기본 투명 (1, 1, 1, 0), 투명이면 그리지 않는다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	/// 배경색 반환.
	/// # 돌려줌
	/// 배경색.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	/// 트리 안에서 오프셋을 정하고 content 노드를 바로 옮긴다. (스크롤 뷰가 없거나 지금 꺼내져 있으면 false)
	/// - 스크롤바가 끌 때, 그리고 씬 코드가 같은 프레임에 바로 옮기고 싶을 때 쓴다.
	/// # 인자
	/// - `node_tree`: 스크롤 뷰가 든 노드 트리.
	/// - `scroll_node_id`: UIScrollView 가 붙은 노드.
	/// - `offset`: 새 오프셋. (픽셀, 그 노드의 지금 content_size 로 보이는 영역을 다시 읽어 범위로 자른다)
	///
	/// # 돌려줌
	/// 정했으면 true. 노드가 없거나 UIScrollView 가 없으면(그 컴포넌트의 tick 중처럼 꺼내져 있을 때도) false.
	///
	/// # 예제
	/// content 노드를 그 자리에서 옮기기
	/// ```rust
	/// use vanilla::{NodeTree, Pivot, UIScrollView, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let scroll_id = node_tree.create_child(root_id, "scroll");
	/// node_tree[scroll_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[scroll_id].set_content_size(Vector2::create(200.0, 100.0));
	/// let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
	/// scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
	/// let content_id = scroll_view.get_content().expect("content");
	///
	/// assert!(UIScrollView::set_scroll_offset_in_tree(&mut node_tree, scroll_id, Vector2::create(0.0, -80.0)));
	/// assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -80.0));
	/// assert!(!UIScrollView::set_scroll_offset_in_tree(&mut node_tree, root_id, Vector2::zero()));
	/// ```
	//==============================================================================
	pub fn set_scroll_offset_in_tree(node_tree: &mut NodeTree, scroll_node_id: NodeId, offset: Vector2) -> bool {
		let viewport_size = match node_tree.get_node(scroll_node_id) {
			Some(scroll_node) => {
				scroll_node.get_content_size()
			}
			None => {
				return false;
			}
		};
		let (content_id, scroll_offset) = match node_tree[scroll_node_id].get_component_mut::<UIScrollView>() {
			Some(scroll_view) => {
				scroll_view.viewport_size = viewport_size;
				scroll_view.apply_scroll_offset(offset);
				scroll_view.is_content_position_sync_pending = false;
				let content_id = scroll_view.get_content();
				let scroll_offset = scroll_view.get_scroll_offset();
				(content_id, scroll_offset)
			}
			None => {
				return false;
			}
		};
		if let Some(content) = content_id.and_then(|content_id| node_tree.get_node_mut(content_id)) {
			content.set_local_position(scroll_offset);
		}
		true
	}

	//==============================================================================
	/// 미뤄 둔 content 노드 위치 / 크기 반영. (tick / 터치 / 휠 때)
	//==============================================================================
	fn sync_content_node(&mut self, context: &mut ComponentContext) {
		if !self.is_content_position_sync_pending && !self.is_content_size_sync_pending {
			return;
		}
		let content_id = match self.get_content() {
			Some(content_id) => {
				content_id
			}
			None => {
				return;
			}
		};
		let scroll_offset = self.get_scroll_offset();
		let scroll_content_size = self.get_scroll_content_size();
		if let Some(content) = context.get_node_tree_mut().get_node_mut(content_id) {
			if self.is_content_position_sync_pending {
				content.set_local_position(scroll_offset);
			}
			if self.is_content_size_sync_pending {
				content.set_content_size(scroll_content_size);
			}
		}
		self.is_content_position_sync_pending = false;
		self.is_content_size_sync_pending = false;
	}

	//==============================================================================
	/// 보이는 영역 크기 읽어 두기. (호스트 content_size)
	//==============================================================================
	fn update_viewport_size(&mut self, context: &ComponentContext) {
		self.viewport_size = context.get_transform().get_content_size();
	}

	//==============================================================================
	/// 오프셋 범위. (최소 x, 최소 y. 최대는 둘 다 0. 끈 축은 0)
	//==============================================================================
	fn calculate_minimum_offset(&self) -> Vector2 {
		let viewport_size = self.viewport_size;
		let scroll_content_size = self.get_scroll_content_size();
		let minimum_x = if self.is_horizontal() {
			(viewport_size.x - scroll_content_size.x).min(0.0)
		}
		else {
			0.0
		};
		let minimum_y = if self.is_vertical() {
			(viewport_size.y - scroll_content_size.y).min(0.0)
		}
		else {
			0.0
		};
		Vector2::create(minimum_x, minimum_y)
	}

	//==============================================================================
	/// 드래그 중 오프셋과 속도 계산. (tick)
	//==============================================================================
	fn update_drag(&mut self, time_delta: f32) {
		let view_input_position = self.current_view_input_position;
		let drag_sensitivity = self.get_drag_sensitivity();
		let raw_delta_x = (view_input_position.x - self.drag_start_view_position.x) * drag_sensitivity;
		let raw_delta_y = (view_input_position.y - self.drag_start_view_position.y) * drag_sensitivity;
		let delta_x = if self.is_horizontal() {
			raw_delta_x
		}
		else {
			0.0
		};
		let delta_y = if self.is_vertical() {
			raw_delta_y
		}
		else {
			0.0
		};
		let proposed_offset = Vector2::create(self.drag_start_offset.x + delta_x, self.drag_start_offset.y + delta_y);
		if self.get_scroll_mode() == ScrollMode::Elastic {
			// 원본처럼 탄성 범위는 끈 축도 계산한다. (끈 축은 delta_x / delta_y 가 0 이라 넘지 않는다)
			let viewport_size = self.viewport_size;
			let scroll_content_size = self.get_scroll_content_size();
			let elastic_maximum_x = 0.0;
			let elastic_minimum_x = (viewport_size.x - scroll_content_size.x).min(0.0);
			let elastic_maximum_y = 0.0;
			let elastic_minimum_y = (viewport_size.y - scroll_content_size.y).min(0.0);
			let mut elastic_offset_x = proposed_offset.x;
			let mut elastic_offset_y = proposed_offset.y;
			if elastic_offset_x > elastic_maximum_x {
				elastic_offset_x = elastic_maximum_x + (elastic_offset_x - elastic_maximum_x) * ELASTIC_RESISTANCE;
			}
			else if elastic_offset_x < elastic_minimum_x {
				elastic_offset_x = elastic_minimum_x + (elastic_offset_x - elastic_minimum_x) * ELASTIC_RESISTANCE;
			}
			if elastic_offset_y > elastic_maximum_y {
				elastic_offset_y = elastic_maximum_y + (elastic_offset_y - elastic_maximum_y) * ELASTIC_RESISTANCE;
			}
			else if elastic_offset_y < elastic_minimum_y {
				elastic_offset_y = elastic_minimum_y + (elastic_offset_y - elastic_minimum_y) * ELASTIC_RESISTANCE;
			}
			self.scroll_offset = Vector2::create(elastic_offset_x, elastic_offset_y);
			self.is_content_position_sync_pending = true;

			if time_delta > 0.0 {
				let raw_velocity_x = (view_input_position.x - self.previous_view_input_position.x) / time_delta;
				let raw_velocity_y = (view_input_position.y - self.previous_view_input_position.y) / time_delta;
				let velocity_x = if self.is_horizontal() {
					raw_velocity_x
				}
				else {
					0.0
				};
				let velocity_y = if self.is_vertical() {
					raw_velocity_y
				}
				else {
					0.0
				};
				self.scroll_velocity = Vector2::create(velocity_x, velocity_y);
			}
		}
		else {
			self.apply_scroll_offset(proposed_offset);
		}
		self.previous_view_input_position = view_input_position;
	}

	//==============================================================================
	/// 탄성 모드에서 놓은 뒤 스프링 / 마찰 물리. (tick)
	//==============================================================================
	fn update_elastic_physics(&mut self, time_delta: f32) {
		let minimum_offset = self.calculate_minimum_offset();
		let bounds_maximum_x = 0.0;
		let bounds_minimum_x = minimum_offset.x;
		let bounds_maximum_y = 0.0;
		let bounds_minimum_y = minimum_offset.y;
		let scroll_velocity = self.get_scroll_velocity();
		let mut velocity_x = scroll_velocity.x;
		let mut velocity_y = scroll_velocity.y;
		let current_scroll_offset = self.get_scroll_offset();
		let mut offset_x = current_scroll_offset.x;
		let mut offset_y = current_scroll_offset.y;
		let clamped_x = math::clamp(offset_x, bounds_minimum_x, bounds_maximum_x);
		let clamped_y = math::clamp(offset_y, bounds_minimum_y, bounds_maximum_y);
		let displacement_x = offset_x - clamped_x;
		let displacement_y = offset_y - clamped_y;
		let is_out_of_bounds = displacement_x != 0.0 || displacement_y != 0.0;
		if is_out_of_bounds {
			let spring_force_x = -displacement_x * SPRING_CONSTANT;
			let spring_force_y = -displacement_y * SPRING_CONSTANT;
			let damping_force_x = -velocity_x * DAMPING_COEFFICIENT;
			let damping_force_y = -velocity_y * DAMPING_COEFFICIENT;
			velocity_x += (spring_force_x + damping_force_x) * time_delta;
			velocity_y += (spring_force_y + damping_force_y) * time_delta;
		}
		else {
			let friction_factor = (1.0 - FRICTION_COEFFICIENT * time_delta).max(0.0);
			velocity_x *= friction_factor;
			velocity_y *= friction_factor;
		}
		offset_x += velocity_x * time_delta;
		offset_y += velocity_y * time_delta;

		// 경계 밖에서 돌아오다 경계를 넘어 안으로 들어오면 경계에서 멈춘다.
		if displacement_x > 0.0 && offset_x < bounds_maximum_x {
			offset_x = bounds_maximum_x;
			velocity_x = 0.0;
		}
		else if displacement_x < 0.0 && offset_x > bounds_minimum_x {
			offset_x = bounds_minimum_x;
			velocity_x = 0.0;
		}
		if displacement_y > 0.0 && offset_y < bounds_maximum_y {
			offset_y = bounds_maximum_y;
			velocity_y = 0.0;
		}
		else if displacement_y < 0.0 && offset_y > bounds_minimum_y {
			offset_y = bounds_minimum_y;
			velocity_y = 0.0;
		}

		let new_clamped_x = math::clamp(offset_x, bounds_minimum_x, bounds_maximum_x);
		let new_clamped_y = math::clamp(offset_y, bounds_minimum_y, bounds_maximum_y);
		let new_displacement_x = offset_x - new_clamped_x;
		let new_displacement_y = offset_y - new_clamped_y;
		let speed_squared = velocity_x * velocity_x + velocity_y * velocity_y;
		if speed_squared < 1.0 && new_displacement_x.abs() < 0.5 && new_displacement_y.abs() < 0.5 {
			offset_x = new_clamped_x;
			offset_y = new_clamped_y;
			velocity_x = 0.0;
			velocity_y = 0.0;
		}
		self.scroll_offset = Vector2::create(offset_x, offset_y);
		self.is_content_position_sync_pending = true;
		self.scroll_velocity = Vector2::create(velocity_x, velocity_y);
	}

	//==============================================================================
	/// 오프셋 적용. (범위로 자른다. content 노드는 다음 반영 때 옮긴다)
	/// - set_scroll_offset 과 달리 다음 tick 에 다시 자르지 않는다. (지금 읽어 둔 보이는 영역 크기로만 자른다)
	///
	/// # 인자
	/// - `offset`: 새 오프셋. (픽셀, content 노드의 로컬 위치. x 는 (보이는 너비 - 콘텐트 너비) ~ 0, y 도 같은 꼴로 자른다. 끈 축은 0)
	//==============================================================================
	pub fn apply_scroll_offset(&mut self, offset: Vector2) {
		let minimum_offset = self.calculate_minimum_offset();
		self.scroll_offset = Vector2::create(math::clamp(offset.x, minimum_offset.x, 0.0), math::clamp(offset.y, minimum_offset.y, 0.0));
		self.is_content_position_sync_pending = true;
	}

	//==============================================================================
	/// 오프셋 설정. (씬 코드용. 다음 tick 에 그때의 영역 크기로 다시 잘라 content 를 옮긴다)
	/// # 인자
	/// - `offset`: 새 오프셋. (픽셀, 아래로 내리면 y 가 음수. 지금 영역 크기로 바로 한 번 자른다)
	///
	/// # 예제
	/// 정한 오프셋은 바로 잘리고 content 는 다음 tick 에 옮겨진다
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, Pivot, UIScrollView, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let scroll_id = node_tree.create_child(root_id, "scroll");
	/// node_tree[scroll_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[scroll_id].set_content_size(Vector2::create(200.0, 100.0));
	/// let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
	/// scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
	/// let content_id = scroll_view.get_content().expect("content");
	///
	/// scroll_view.set_scroll_offset(Vector2::create(-50.0, -120.0));
	/// assert_eq!(scroll_view.get_scroll_offset(), Vector2::create(0.0, -120.0));
	/// assert_eq!(node_tree[content_id].get_local_position(), Vector2::zero());
	/// node_tree.tick(&engine, 0.016);
	/// assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -120.0));
	/// ```
	//==============================================================================
	pub fn set_scroll_offset(&mut self, offset: Vector2) {
		self.apply_scroll_offset(offset);
		self.is_offset_clamp_pending = true;
	}

	//==============================================================================
	/// 오프셋 반환. (content 노드의 로컬 위치, 아래로 스크롤하면 y 가 음수)
	/// # 돌려줌
	/// 지금 오프셋. (픽셀, 0 이하)
	//==============================================================================
	pub fn get_scroll_offset(&self) -> Vector2 {
		self.scroll_offset
	}

	//==============================================================================
	/// 가로 스크롤 켜기 설정.
	/// # 인자
	/// - `is_horizontal`: true 면 가로로 끌고 굴릴 수 있다. (기본 true) false 면 오프셋 x 를 0 으로 자른다.
	//==============================================================================
	pub fn set_horizontal(&mut self, is_horizontal: bool) {
		self.horizontal_enabled = is_horizontal;
	}

	//==============================================================================
	/// 가로 스크롤 켜짐 여부.
	/// # 돌려줌
	/// 가로 스크롤이 켜져 있으면 true.
	//==============================================================================
	pub fn is_horizontal(&self) -> bool {
		self.horizontal_enabled
	}

	//==============================================================================
	/// 세로 스크롤 켜기 설정.
	/// # 인자
	/// - `is_vertical`: true 면 세로로 끌고 굴릴 수 있다. (기본 true) false 면 오프셋 y 를 0 으로 자른다.
	//==============================================================================
	pub fn set_vertical(&mut self, is_vertical: bool) {
		self.vertical_enabled = is_vertical;
	}

	//==============================================================================
	/// 세로 스크롤 켜짐 여부.
	/// # 돌려줌
	/// 세로 스크롤이 켜져 있으면 true.
	//==============================================================================
	pub fn is_vertical(&self) -> bool {
		self.vertical_enabled
	}

	//==============================================================================
	/// 스크롤하지 않는 축으로 끄는 손가락을 누른 노드에 남길지 설정. (기본 false, TouchRecognizer 가 읽는다)
	/// # 인자
	/// - `passes_cross_axis_drag`: true 면 한 축만 켠 스크롤 뷰가 다른 축 방향 드래그를 가져가지 않는다. (is_scroll_drag)
	//==============================================================================
	pub fn set_passes_cross_axis_drag(&mut self, passes_cross_axis_drag: bool) {
		self.passes_cross_axis_drag = passes_cross_axis_drag;
	}

	//==============================================================================
	/// 스크롤하지 않는 축으로 끄는 손가락을 누른 노드에 남기는지.
	/// # 돌려줌
	/// 남기는 옵션이 켜져 있으면 true.
	//==============================================================================
	pub fn is_passing_cross_axis_drag(&self) -> bool {
		self.passes_cross_axis_drag
	}

	//==============================================================================
	/// 끈 방향이 이 뷰가 가져갈 드래그인지. (스크롤하지 않는 축의 드래그를 남기는 옵션을 켰을 때만 가려낸다)
	/// # 인자
	/// - `delta_x`: 끈 거리 x. (픽셀, 부호는 보지 않는다)
	/// - `delta_y`: 끈 거리 y. (픽셀, 부호는 보지 않는다)
	///
	/// # 돌려줌
	/// 가져갈 드래그면 true. 옵션이 꺼져 있거나 두 축이 다 켜져 있으면 늘 true. 세로만 켰으면 |y| >= |x| 일 때, 가로만 켰으면 |x| >= |y| 일 때 true. 두 축이 다 꺼져 있으면 false.
	///
	/// # 예제
	/// 세로 목록이 가로 드래그를 남기기
	/// ```rust
	/// use vanilla::UIScrollView;
	///
	/// let mut scroll_view = UIScrollView::new();
	/// scroll_view.set_horizontal(false);
	/// assert!(scroll_view.is_scroll_drag(30.0, 2.0));
	///
	/// scroll_view.set_passes_cross_axis_drag(true);
	/// assert!(!scroll_view.is_scroll_drag(30.0, 2.0));
	/// assert!(scroll_view.is_scroll_drag(2.0, -30.0));
	/// ```
	//==============================================================================
	pub fn is_scroll_drag(&self, delta_x: f32, delta_y: f32) -> bool {
		if !self.passes_cross_axis_drag {
			return true;
		}
		if self.horizontal_enabled && self.vertical_enabled {
			return true;
		}
		let absolute_x = delta_x.abs();
		let absolute_y = delta_y.abs();
		if self.vertical_enabled {
			return absolute_y >= absolute_x;
		}
		if self.horizontal_enabled {
			return absolute_x >= absolute_y;
		}
		false
	}

	//==============================================================================
	/// 드래그 반영량 배율 설정. (1 = 기본, 2 = 두 배 빠르게)
	/// # 인자
	/// - `drag_sensitivity`: 끈 거리에 곱할 배율. (기본 1)
	//==============================================================================
	pub fn set_drag_sensitivity(&mut self, drag_sensitivity: f32) {
		self.drag_sensitivity = drag_sensitivity;
	}

	//==============================================================================
	/// 드래그 반영량 배율 반환.
	/// # 돌려줌
	/// 드래그 배율.
	//==============================================================================
	pub fn get_drag_sensitivity(&self) -> f32 {
		self.drag_sensitivity
	}

	//==============================================================================
	/// 스크롤 모드 설정.
	/// # 인자
	/// - `scroll_mode`: 경계 처리 모드. (기본 ScrollMode::Clamp)
	//==============================================================================
	pub fn set_scroll_mode(&mut self, scroll_mode: ScrollMode) {
		self.scroll_mode = scroll_mode;
	}

	//==============================================================================
	/// 스크롤 모드 반환.
	/// # 돌려줌
	/// 지금 스크롤 모드.
	//==============================================================================
	pub fn get_scroll_mode(&self) -> ScrollMode {
		self.scroll_mode
	}

	//==============================================================================
	/// 스크롤 콘텐츠 크기 설정. (보이는 영역보다 커야 스크롤된다. content 노드 크기는 다음 tick 에 맞춘다)
	/// # 인자
	/// - `scroll_content_size`: 스크롤할 콘텐트 전체 크기. (픽셀, 기본 (0, 0))
	//==============================================================================
	pub fn set_scroll_content_size(&mut self, scroll_content_size: Vector2) {
		self.scroll_content_size = scroll_content_size;
		self.is_content_size_sync_pending = true;
	}

	//==============================================================================
	/// 스크롤 콘텐츠 크기 반환.
	/// # 돌려줌
	/// 콘텐트 전체 크기. (픽셀)
	//==============================================================================
	pub fn get_scroll_content_size(&self) -> Vector2 {
		self.scroll_content_size
	}

	//==============================================================================
	/// 드래그 중인지.
	/// # 돌려줌
	/// 눌러 끄는 중이면 true. (떼거나 취소하면 false)
	//==============================================================================
	pub fn is_dragging(&self) -> bool {
		self.is_dragging
	}

	//==============================================================================
	/// 지금 스크롤 속도 반환.
	/// # 돌려줌
	/// 스크롤 속도. (픽셀/초) 탄성 모드에서만 셈하고, 클램프 모드는 누를 때 0 으로 비운 뒤 그대로다.
	//==============================================================================
	pub fn get_scroll_velocity(&self) -> Vector2 {
		self.scroll_velocity
	}

	//==============================================================================
	/// 세로 스크롤바 보이기 설정. (true 면 다음 tick 에 호스트의 자식으로 만들어 켠다)
	/// # 인자
	/// - `shows`: true 면 보이고 false 면 이미 만든 스크롤바 노드를 끈다. (기본 false)
	///
	/// # 예제
	/// 첫 tick 뒤에 생기는 세로 스크롤바
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, Pivot, UIScrollView, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let scroll_id = node_tree.create_child(root_id, "scroll");
	/// node_tree[scroll_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[scroll_id].set_content_size(Vector2::create(200.0, 100.0));
	/// let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
	/// scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
	/// scroll_view.set_shows_vertical_scroll_bar(true);
	/// assert!(scroll_view.get_vertical_scroll_bar().is_none());
	///
	/// node_tree.tick(&engine, 0.016);
	/// let scroll_view = node_tree[scroll_id].get_component::<UIScrollView>().expect("scroll view");
	/// let scroll_bar_id = scroll_view.get_vertical_scroll_bar().expect("scroll bar");
	/// assert_eq!(scroll_view.get_inner_content_size(), Vector2::create(176.0, 100.0));
	/// assert_eq!(node_tree[scroll_bar_id].get_content_size(), Vector2::create(12.0, 88.0));
	/// ```
	//==============================================================================
	pub fn set_shows_vertical_scroll_bar(&mut self, shows: bool) {
		self.shows_vertical_scroll_bar = shows;
		self.pending_vertical_scroll_bar_active = Some(shows);
	}

	//==============================================================================
	/// 가로 스크롤바 보이기 설정.
	/// - 세로와 같은 꼴이다. (true 면 다음 tick 에 호스트의 자식으로 만들어 켠다)
	///
	/// # 인자
	/// - `shows`: true 면 보이고 false 면 이미 만든 스크롤바 노드를 끈다. (기본 false)
	//==============================================================================
	pub fn set_shows_horizontal_scroll_bar(&mut self, shows: bool) {
		self.shows_horizontal_scroll_bar = shows;
		self.pending_horizontal_scroll_bar_active = Some(shows);
	}

	//==============================================================================
	/// 세로 스크롤바 보이기 설정을 그 자리에서 노드에 건다. (원본 setShowsVerticalScrollBar 처럼 없으면 호스트의 자식으로 만들고 켜거나 끈다, 스크롤바 노드 핸들)
	/// # 인자
	/// - `node_tree`: 스크롤 뷰가 든 노드 트리.
	/// - `scroll_node_id`: UIScrollView 가 붙은 노드.
	/// - `shows`: true 면 만들어 켜고 false 면 있는 노드를 끈다.
	///
	/// # 돌려줌
	/// 세로 스크롤바 노드. 노드나 UIScrollView 가 없으면 None. 끄는데 아직 만든 노드가 없어도 None.
	///
	/// # 예제
	/// tick 없이 바로 만들고 끄기
	/// ```rust
	/// use vanilla::{NodeTree, Pivot, UIScrollBar, UIScrollView, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let scroll_id = node_tree.create_child(root_id, "scroll");
	/// node_tree[scroll_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[scroll_id].set_content_size(Vector2::create(200.0, 100.0));
	/// node_tree.add_component(scroll_id, UIScrollView::new());
	///
	/// let scroll_bar_id = UIScrollView::set_shows_vertical_scroll_bar_in_tree(&mut node_tree, scroll_id, true).expect("scroll bar");
	/// assert_eq!(node_tree.get_parent_id(scroll_bar_id), Some(scroll_id));
	/// assert!(node_tree[scroll_bar_id].get_component::<UIScrollBar>().is_some());
	/// assert!(node_tree[scroll_bar_id].is_active());
	///
	/// assert_eq!(UIScrollView::set_shows_vertical_scroll_bar_in_tree(&mut node_tree, scroll_id, false), Some(scroll_bar_id));
	/// assert!(!node_tree[scroll_bar_id].is_active());
	/// assert!(UIScrollView::set_shows_horizontal_scroll_bar_in_tree(&mut node_tree, scroll_id, false).is_none());
	/// ```
	//==============================================================================
	pub fn set_shows_vertical_scroll_bar_in_tree(node_tree: &mut NodeTree, scroll_node_id: NodeId, shows: bool) -> Option<NodeId> {
		UIScrollView::set_shows_scroll_bar_in_tree(node_tree, scroll_node_id, ScrollBarAxis::Vertical, shows)
	}

	//==============================================================================
	/// 가로 스크롤바 보이기 설정을 그 자리에서 노드에 건다. (원본 setShowsHorizontalScrollBar)
	/// - 세로와 같은 꼴이다. (없으면 호스트의 자식으로 만들고 켜거나 끈다)
	///
	/// # 인자
	/// - `node_tree`: 스크롤 뷰가 든 노드 트리.
	/// - `scroll_node_id`: UIScrollView 가 붙은 노드.
	/// - `shows`: true 면 만들어 켜고 false 면 있는 노드를 끈다.
	///
	/// # 돌려줌
	/// 가로 스크롤바 노드. 노드나 UIScrollView 가 없으면 None. 끄는데 아직 만든 노드가 없어도 None.
	//==============================================================================
	pub fn set_shows_horizontal_scroll_bar_in_tree(node_tree: &mut NodeTree, scroll_node_id: NodeId, shows: bool) -> Option<NodeId> {
		UIScrollView::set_shows_scroll_bar_in_tree(node_tree, scroll_node_id, ScrollBarAxis::Horizontal, shows)
	}

	//==============================================================================
	/// 한 축의 스크롤바 보이기 설정을 그 자리에서 노드에 건다. (보이기면 노드를 만들고 미뤄 둔 켜기 / 끄기를 바로 반영한다)
	//==============================================================================
	fn set_shows_scroll_bar_in_tree(node_tree: &mut NodeTree, scroll_node_id: NodeId, axis: ScrollBarAxis, shows: bool) -> Option<NodeId> {
		let scroll_bar_id = {
			let scroll_view = node_tree.get_node_mut(scroll_node_id)?.get_component_mut::<UIScrollView>()?;
			match axis {
				ScrollBarAxis::Vertical => {
					scroll_view.set_shows_vertical_scroll_bar(shows);
					scroll_view.vertical_scroll_bar_id
				}
				ScrollBarAxis::Horizontal => {
					scroll_view.set_shows_horizontal_scroll_bar(shows);
					scroll_view.horizontal_scroll_bar_id
				}
			}
		};
		let scroll_bar_id = match scroll_bar_id {
			Some(scroll_bar_id) => {
				scroll_bar_id
			}
			None => {
				if !shows {
					return None;
				}
				let scroll_bar_name = match axis {
					ScrollBarAxis::Vertical => {
						"verticalScrollBar"
					}
					ScrollBarAxis::Horizontal => {
						"horizontalScrollBar"
					}
				};
				UIScrollView::create_scroll_bar(node_tree, scroll_node_id, scroll_bar_name, axis)
			}
		};
		if let Some(scroll_view) = node_tree[scroll_node_id].get_component_mut::<UIScrollView>() {
			match axis {
				ScrollBarAxis::Vertical => {
					scroll_view.vertical_scroll_bar_id = Some(scroll_bar_id);
					scroll_view.pending_vertical_scroll_bar_active = None;
				}
				ScrollBarAxis::Horizontal => {
					scroll_view.horizontal_scroll_bar_id = Some(scroll_bar_id);
					scroll_view.pending_horizontal_scroll_bar_active = None;
				}
			}
		}
		if let Some(scroll_bar_node) = node_tree.get_node_mut(scroll_bar_id) {
			scroll_bar_node.set_active(shows);
		}
		Some(scroll_bar_id)
	}

	//==============================================================================
	/// 세로 스크롤바 노드 핸들. (꾸밀 때 `node_tree[id].get_component_mut::<UIScrollBar>()`)
	/// # 돌려줌
	/// 세로 스크롤바 노드. 아직 만들지 않았으면 None. (set_shows_vertical_scroll_bar 뒤 첫 tick 이 지나야 생긴다)
	//==============================================================================
	pub fn get_vertical_scroll_bar(&self) -> Option<NodeId> {
		self.vertical_scroll_bar_id
	}

	//==============================================================================
	/// 가로 스크롤바 노드 핸들.
	/// # 돌려줌
	/// 가로 스크롤바 노드. 아직 만들지 않았으면 None.
	//==============================================================================
	pub fn get_horizontal_scroll_bar(&self) -> Option<NodeId> {
		self.horizontal_scroll_bar_id
	}

	//==============================================================================
	/// 세로 스크롤바가 차지하는 폭. (보이는 중일 때만)
	/// # 돌려줌
	/// 보이기로 정했고 노드가 있으면 24 (두께 12 + 여백 6 의 두 배, 픽셀), 아니면 0.
	//==============================================================================
	pub fn get_vertical_scroll_bar_reserved_width(&self) -> f32 {
		if self.shows_vertical_scroll_bar && self.vertical_scroll_bar_id.is_some() {
			SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN * 2.0
		}
		else {
			0.0
		}
	}

	//==============================================================================
	/// 가로 스크롤바가 차지하는 높이. (보이는 중일 때만)
	/// # 돌려줌
	/// 보이기로 정했고 노드가 있으면 24 (두께 12 + 여백 6 의 두 배, 픽셀), 아니면 0.
	//==============================================================================
	pub fn get_horizontal_scroll_bar_reserved_height(&self) -> f32 {
		if self.shows_horizontal_scroll_bar && self.horizontal_scroll_bar_id.is_some() {
			SCROLLBAR_THICKNESS + SCROLLBAR_MARGIN * 2.0
		}
		else {
			0.0
		}
	}

	//==============================================================================
	/// 스크롤바를 뺀, 콘텐트가 그려질 수 있는 보이는 영역 크기.
	/// # 돌려줌
	/// 보이는 영역에서 스크롤바 자리를 뺀 크기. (픽셀, 0 아래로 내려가지 않는다. 보이는 영역은 붙을 때와 tick, 터치, 휠 때 읽어 둔 값)
	//==============================================================================
	pub fn get_inner_content_size(&self) -> Vector2 {
		let viewport_size = self.viewport_size;
		let reserved_x = self.get_vertical_scroll_bar_reserved_width();
		let reserved_y = self.get_horizontal_scroll_bar_reserved_height();
		Vector2::create((viewport_size.x - reserved_x).max(0.0), (viewport_size.y - reserved_y).max(0.0))
	}

	//==============================================================================
	/// 휠 스크롤 배율 설정.
	/// # 인자
	/// - `wheel_scroll_scale`: 휠 값에 곱할 배율. (기본 1)
	//==============================================================================
	pub fn set_wheel_scroll_scale(&mut self, wheel_scroll_scale: f32) {
		self.wheel_scroll_scale = wheel_scroll_scale;
	}

	//==============================================================================
	/// 스크롤바 노드 하나 만들기. (호스트의 자식)
	//==============================================================================
	fn create_scroll_bar(node_tree: &mut NodeTree, scroll_node_id: NodeId, name: &str, axis: ScrollBarAxis) -> NodeId {
		let scroll_bar_id = node_tree.create_child(scroll_node_id, name);
		let mut scroll_bar = UIScrollBar::new();
		scroll_bar.set_axis(axis);
		scroll_bar.set_scroll_view(Some(scroll_node_id));
		node_tree.add_component(scroll_bar_id, scroll_bar);
		scroll_bar_id
	}

	//==============================================================================
	/// 보이기로 정한 스크롤바를 만들고, 미뤄 둔 켜기 / 끄기를 반영한다.
	//==============================================================================
	fn update_scroll_bar_nodes(&mut self, context: &mut ComponentContext) {
		let scroll_node_id = context.get_node_id();
		let node_tree = context.get_node_tree_mut();
		if self.shows_vertical_scroll_bar && self.vertical_scroll_bar_id.is_none() {
			let scroll_bar_id = UIScrollView::create_scroll_bar(node_tree, scroll_node_id, "verticalScrollBar", ScrollBarAxis::Vertical);
			self.vertical_scroll_bar_id = Some(scroll_bar_id);
		}
		if self.shows_horizontal_scroll_bar && self.horizontal_scroll_bar_id.is_none() {
			let scroll_bar_id = UIScrollView::create_scroll_bar(node_tree, scroll_node_id, "horizontalScrollBar", ScrollBarAxis::Horizontal);
			self.horizontal_scroll_bar_id = Some(scroll_bar_id);
		}
		if let Some(is_active) = self.pending_vertical_scroll_bar_active.take() {
			if let Some(scroll_bar) = self.vertical_scroll_bar_id.and_then(|scroll_bar_id| node_tree.get_node_mut(scroll_bar_id)) {
				scroll_bar.set_active(is_active);
			}
		}
		if let Some(is_active) = self.pending_horizontal_scroll_bar_active.take() {
			if let Some(scroll_bar) = self.horizontal_scroll_bar_id.and_then(|scroll_bar_id| node_tree.get_node_mut(scroll_bar_id)) {
				scroll_bar.set_active(is_active);
			}
		}
	}

	//==============================================================================
	/// 매 tick 스크롤바 위치 / 크기 갱신.
	//==============================================================================
	fn layout_scroll_bars(&mut self, context: &mut ComponentContext) {
		let viewport_size = self.viewport_size;
		let reserved_x = self.get_vertical_scroll_bar_reserved_width();
		let reserved_y = self.get_horizontal_scroll_bar_reserved_height();
		let shows_vertical_scroll_bar = self.shows_vertical_scroll_bar;
		let shows_horizontal_scroll_bar = self.shows_horizontal_scroll_bar;
		let node_tree = context.get_node_tree_mut();
		if shows_vertical_scroll_bar {
			if let Some(scroll_bar) = self.vertical_scroll_bar_id.and_then(|scroll_bar_id| node_tree.get_node_mut(scroll_bar_id)) {
				let position_x = viewport_size.x - SCROLLBAR_THICKNESS - SCROLLBAR_MARGIN;
				let position_y = SCROLLBAR_MARGIN;
				let height = (viewport_size.y - SCROLLBAR_MARGIN * 2.0 - reserved_y).max(0.0);
				scroll_bar.set_local_position(Vector2::create(position_x, position_y));
				scroll_bar.set_content_size(Vector2::create(SCROLLBAR_THICKNESS, height));
			}
		}
		if shows_horizontal_scroll_bar {
			if let Some(scroll_bar) = self.horizontal_scroll_bar_id.and_then(|scroll_bar_id| node_tree.get_node_mut(scroll_bar_id)) {
				let position_x = SCROLLBAR_MARGIN;
				let position_y = viewport_size.y - SCROLLBAR_THICKNESS - SCROLLBAR_MARGIN;
				let width = (viewport_size.x - SCROLLBAR_MARGIN * 2.0 - reserved_x).max(0.0);
				scroll_bar.set_local_position(Vector2::create(position_x, position_y));
				scroll_bar.set_content_size(Vector2::create(width, SCROLLBAR_THICKNESS));
			}
		}
	}
}

impl Default for UIScrollView {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> UIScrollView {
		UIScrollView::new()
	}
}

#[crate::reflect]
impl Component for UIScrollView {
	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (품은 뷰의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (변경용, 품은 뷰의 것)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름. ("ScrollView")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ScrollView"
	}

	//==============================================================================
	/// 휠을 받는지. (늘 true)
	//==============================================================================
	#[reflect(skip)]
	fn is_wheel_receiver(&self) -> bool {
		true
	}

	//==============================================================================
	/// 붙음. (보이는 영역 크기를 읽어 둔다)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		self.viewport_size = transform.get_content_size();
	}

	//==============================================================================
	/// 트리에 붙음. (content 노드, Mask, interactable)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
		let scroll_node = &mut node_tree[node_id];
		if !scroll_node.has_component::<Mask>() {
			scroll_node.add_component(Mask::new());
		}
		scroll_node.set_interactable(true);
	}

	//==============================================================================
	/// 갱신. (스크롤바 → 드래그 → 탄성 물리 → content 노드 반영)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.update_viewport_size(context);
		if self.is_offset_clamp_pending {
			self.is_offset_clamp_pending = false;
			let scroll_offset = self.scroll_offset;
			self.apply_scroll_offset(scroll_offset);
		}

		// 스크롤바 자동 레이아웃.
		self.update_scroll_bar_nodes(context);
		self.layout_scroll_bars(context);

		// 드래그 중 오프셋과 속도.
		if self.is_dragging() {
			self.update_drag(time_delta);
		}

		// 탄성 모드에서 놓은 뒤.
		if self.get_scroll_mode() == ScrollMode::Elastic && !self.is_dragging() {
			self.update_elastic_physics(time_delta);
		}
		self.sync_content_node(context);
	}

	//==============================================================================
	/// 출력. (배경)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
	}

	//==============================================================================
	/// 내용 영역 안에 그리는지. (늘 true)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 터치 누름. (드래그 시작, 속도를 비운다)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.update_viewport_size(context);
		self.is_dragging = true;
		self.drag_start_view_position = view_input_position;
		self.drag_start_offset = self.get_scroll_offset();
		self.scroll_velocity = Vector2::zero();
		self.previous_view_input_position = view_input_position;
		self.current_view_input_position = view_input_position;
	}

	//==============================================================================
	/// 터치 이동. (자리만 적어 두고 tick 에서 오프셋을 셈한다)
	//==============================================================================
	fn touch_move(&mut self, _context: &mut ComponentContext, view_input_position: Vector2) {
		if self.is_dragging() {
			self.current_view_input_position = view_input_position;
		}
	}

	//==============================================================================
	/// 터치 뗌. (드래그 끝)
	//==============================================================================
	fn touch_release(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.is_dragging = false;
	}

	//==============================================================================
	/// 터치 취소. (드래그 끝)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.is_dragging = false;
	}

	//==============================================================================
	/// 휠. (DOM WheelEvent 와 같은 부호: 아래로 굴리면 +y → 콘텐트가 위로. 늘 받아 쓴다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		self.update_viewport_size(context);
		let offset = self.get_scroll_offset();
		let delta_x = if self.is_horizontal() {
			wheel_delta.x * self.wheel_scroll_scale
		}
		else {
			0.0
		};
		let delta_y = if self.is_vertical() {
			wheel_delta.y * self.wheel_scroll_scale
		}
		else {
			0.0
		};
		self.apply_scroll_offset(Vector2::create(offset.x - delta_x, offset.y - delta_y));
		// 휠 다음에는 탄성 모드의 남은 속도를 비워 튕기지 않게 한다.
		self.scroll_velocity = Vector2::zero();
		self.sync_content_node(context);
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	/// 시험 장면. (왼쪽 위 (100, 100), 크기 200 x 100 인 스크롤 뷰, 콘텐트 200 x 400)
	//==============================================================================
	fn create_scroll_tree() -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let scroll_id = node_tree.create_child(root_id, "scroll");
		let scroll_node = &mut node_tree[scroll_id];
		scroll_node.set_pivot(Pivot::TOP_LEFT);
		scroll_node.set_local_position(Vector2::create(100.0, 100.0));
		scroll_node.set_content_size(Vector2::create(200.0, 100.0));
		let scroll_view = node_tree.add_component(scroll_id, UIScrollView::new()).expect("scroll view");
		scroll_view.set_scroll_content_size(Vector2::create(200.0, 400.0));
		(node_tree, scroll_id)
	}

	//==============================================================================
	/// 스크롤 뷰 반환.
	//==============================================================================
	fn get_scroll_view(node_tree: &mut NodeTree, scroll_id: NodeId) -> &mut UIScrollView {
		node_tree[scroll_id].get_component_mut::<UIScrollView>().expect("scroll view")
	}

	//==============================================================================
	/// 붙이면 content, Mask, interactable 이 생기고 tick 에 content 크기가 맞춰진다.
	//==============================================================================
	#[test]
	fn attach_creates_content_and_mask() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		assert!(node_tree[scroll_id].has_component::<Mask>());
		assert!(node_tree[scroll_id].is_interactable());
		let content_id = get_scroll_view(&mut node_tree, scroll_id).get_content().expect("content");
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[content_id].get_content_size(), Vector2::create(200.0, 400.0));
	}

	//==============================================================================
	/// 오프셋은 [보이는 크기 - 콘텐트 크기, 0] 으로 잘린다. 끈 축은 0.
	//==============================================================================
	#[test]
	fn offset_clamping() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		{
			let scroll_view = get_scroll_view(&mut node_tree, scroll_id);
			scroll_view.set_scroll_offset(Vector2::create(-50.0, -1000.0));
			assert_eq!(scroll_view.get_scroll_offset(), Vector2::create(0.0, -300.0));
			scroll_view.set_scroll_offset(Vector2::create(0.0, 30.0));
			assert_eq!(scroll_view.get_scroll_offset(), Vector2::create(0.0, 0.0));
			scroll_view.set_scroll_offset(Vector2::create(0.0, -120.0));
		}
		let content_id = get_scroll_view(&mut node_tree, scroll_id).get_content().expect("content");
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -120.0));

		// 세로를 끄면 y 는 0 으로 잘린다.
		get_scroll_view(&mut node_tree, scroll_id).set_vertical(false);
		get_scroll_view(&mut node_tree, scroll_id).set_scroll_offset(Vector2::create(0.0, -120.0));
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::zero());

		// 트리 안에서 정하면 content 가 바로 옮겨진다.
		get_scroll_view(&mut node_tree, scroll_id).set_vertical(true);
		assert!(UIScrollView::set_scroll_offset_in_tree(&mut node_tree, scroll_id, Vector2::create(0.0, -80.0)));
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -80.0));
	}

	//==============================================================================
	/// 그 자리의 휠은 가장 앞 스크롤 칸이 받는다. (칸을 누를 수 없게 해 두어도, 칸 안의 누를 수 없는 글 위에서도) 칸을 덮은 누를 수 있는 막은 휠을 가린다.
	//==============================================================================
	#[test]
	fn wheel_goes_to_frontmost_scroll_view() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		node_tree[scroll_id].set_interactable(false);
		let content_id = get_scroll_view(&mut node_tree, scroll_id).get_content().expect("content");
		node_tree.tick(&engine, 0.016);
		let label_id = node_tree.create_child(content_id, "label");
		node_tree[label_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[label_id].set_rect(10.0, 10.0, 50.0, 20.0);
		let view_position = Vector2::create(120.0, 120.0);
		assert_eq!(node_tree.find_topmost_interactable_node(view_position), None);
		assert_eq!(node_tree.find_wheel_target_node(view_position), Some(scroll_id));
		assert!(node_tree.dispatch_wheel_at(view_position, &engine, Vector2::create(0.0, 40.0)));
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, -40.0));

		let root_id = node_tree.get_root_id();
		let cover_id = node_tree.create_child(root_id, "cover");
		node_tree[cover_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[cover_id].set_rect(0.0, 0.0, 800.0, 600.0);
		node_tree[cover_id].set_interactable(true);
		assert!(!node_tree.dispatch_wheel_at(view_position, &engine, Vector2::create(0.0, 40.0)));
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, -40.0));
	}

	//==============================================================================
	/// 휠은 content 의 자식에서 스크롤 뷰까지 올라와 받아 쓰이고, 경계에서 멈춘다.
	//==============================================================================
	#[test]
	fn wheel_scrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		let content_id = get_scroll_view(&mut node_tree, scroll_id).get_content().expect("content");
		let item_id = node_tree.create_child(content_id, "item");
		let is_handled = node_tree.dispatch_wheel(item_id, &engine, Vector2::create(0.0, 120.0));
		assert!(is_handled);
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, -120.0));
		assert_eq!(node_tree[content_id].get_local_position(), Vector2::create(0.0, -120.0));
		node_tree.dispatch_wheel(item_id, &engine, Vector2::create(0.0, 1000.0));
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, -300.0));
		node_tree.dispatch_wheel(item_id, &engine, Vector2::create(0.0, -1000.0));
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::zero());
	}

	//==============================================================================
	/// 누르고 끌면 tick 에서 끈 만큼 움직인다.
	//==============================================================================
	#[test]
	fn drag_scrolls() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Press, Vector2::create(150.0, 180.0));
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Moved, Vector2::create(150.0, 140.0));
		node_tree.tick(&engine, 0.016);
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, -40.0));
		assert!(get_scroll_view(&mut node_tree, scroll_id).is_dragging());
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Release, Vector2::create(150.0, 140.0));
		assert!(!get_scroll_view(&mut node_tree, scroll_id).is_dragging());
	}

	//==============================================================================
	/// 탄성 모드: 끌어서 경계를 넘으면 절반만 따라오고, 놓으면 경계로 돌아온다.
	//==============================================================================
	#[test]
	fn elastic_springs_back() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		get_scroll_view(&mut node_tree, scroll_id).set_scroll_mode(ScrollMode::Elastic);
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Press, Vector2::create(150.0, 120.0));
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Moved, Vector2::create(150.0, 180.0));
		node_tree.tick(&engine, 0.016);
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::create(0.0, 30.0));
		node_tree.dispatch_touch(scroll_id, &engine, TouchKind::Release, Vector2::create(150.0, 180.0));
		for _ in 0..300 {
			node_tree.tick(&engine, 0.016);
		}
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_scroll_offset(), Vector2::zero());
	}

	//==============================================================================
	/// 세로 스크롤바: 첫 tick 에 호스트의 자식으로 만들어져 오른쪽에 놓인다.
	//==============================================================================
	#[test]
	fn vertical_scroll_bar_is_created_and_laid_out() {
		let engine = Engine::new(EngineConfiguration::new());
		let (mut node_tree, scroll_id) = create_scroll_tree();
		get_scroll_view(&mut node_tree, scroll_id).set_shows_vertical_scroll_bar(true);
		assert!(get_scroll_view(&mut node_tree, scroll_id).get_vertical_scroll_bar().is_none());
		node_tree.tick(&engine, 0.016);
		let scroll_bar_id = get_scroll_view(&mut node_tree, scroll_id).get_vertical_scroll_bar().expect("scroll bar");
		assert_eq!(node_tree.get_parent_id(scroll_bar_id), Some(scroll_id));
		assert_eq!(node_tree[scroll_bar_id].get_local_position(), Vector2::create(200.0 - 12.0 - 6.0, 6.0));
		assert_eq!(node_tree[scroll_bar_id].get_content_size(), Vector2::create(12.0, 88.0));
		assert!(node_tree[scroll_bar_id].is_active());
		assert_eq!(get_scroll_view(&mut node_tree, scroll_id).get_inner_content_size(), Vector2::create(176.0, 100.0));

		get_scroll_view(&mut node_tree, scroll_id).set_shows_vertical_scroll_bar(false);
		node_tree.tick(&engine, 0.016);
		assert!(!node_tree[scroll_bar_id].is_active());

		// 원본처럼 그 자리에서: 첫 tick 전에도 가로 스크롤바 노드가 생기고 켜진다. 끄면 바로 꺼진다.
		let (mut immediate_tree, immediate_scroll_id) = create_scroll_tree();
		let horizontal_bar_id = UIScrollView::set_shows_horizontal_scroll_bar_in_tree(&mut immediate_tree, immediate_scroll_id, true).expect("horizontal bar");
		assert_eq!(get_scroll_view(&mut immediate_tree, immediate_scroll_id).get_horizontal_scroll_bar(), Some(horizontal_bar_id));
		assert_eq!(immediate_tree.get_parent_id(horizontal_bar_id), Some(immediate_scroll_id));
		assert!(immediate_tree[horizontal_bar_id].is_active());
		assert_eq!(UIScrollView::set_shows_horizontal_scroll_bar_in_tree(&mut immediate_tree, immediate_scroll_id, false), Some(horizontal_bar_id));
		assert!(!immediate_tree[horizontal_bar_id].is_active());
		assert!(UIScrollView::set_shows_vertical_scroll_bar_in_tree(&mut immediate_tree, immediate_scroll_id, false).is_none());
		immediate_tree.tick(&engine, 0.016);
		assert!(!immediate_tree[horizontal_bar_id].is_active());
		assert!(get_scroll_view(&mut immediate_tree, immediate_scroll_id).get_vertical_scroll_bar().is_none());
	}
}
