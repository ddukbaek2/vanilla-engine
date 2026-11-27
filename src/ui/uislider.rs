//==============================================================================
//! 슬라이더. (원본 ui/uislider.js 의 UISlider)
//! - 끌거나 눌러서 값을 고른다. 트랙 + 채움 + 손잡이(원)를 직접 그린다. 값은 범위로 자르기만 한다. (원본에도 눈금 맞추기는 없다)
//! - 손잡이 노드(set_thumb_node)를 정하면 원을 그리지 않고 그 노드를 손잡이 자리로 옮긴다.
//! - 원본과 다른 것:
//!   - 원본은 draw 안에서 손잡이 노드를 옮긴다. 러스트 판은 그리는 동안 트리를 바꿀 수 없어 tick 에서 옮긴다. (손잡이 노드는 호스트의 자식으로 둔다)
//!   - 입력 좌표는 원본처럼 호스트 왼쪽 위 모서리 기준이되, 노드 행렬의 역으로 바꿔 크기와 회전까지 따른다. (원본은 모서리 위치만 뺀다)
//!   - 값 바뀜 이벤트는 슬라이더와 문맥을 받는다. 씬 코드에서 부르는 fire_value_changed 는 다음 tick 에 이벤트를 부른다.
//!     받은 이벤트는 `Rc<RefCell>` 에 담는다. get_value_changed_event 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow_mut 로 빌려 부른다.
//! - attach_to_tree 에서 호스트 노드를 interactable 로 켠다. node_tree.add_component(node_id, UISlider::new()) 로 붙인다.
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
use std::cell::RefCell;
use std::rc::Rc;

//==============================================================================
/// 슬라이더 이벤트. (슬라이더와 문맥을 받는다)
//==============================================================================
pub type UISliderEvent = Box<dyn FnMut(&mut UISlider, &mut ComponentContext)>;

//==============================================================================
/// 슬라이더.
///
/// # 예제
/// 음량 슬라이더 만들기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, Pivot, TouchKind, UISlider, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let slider_id = node_tree.create_child(root_id, "VolumeSlider");
/// node_tree[slider_id].set_pivot(Pivot::TOP_LEFT);
/// node_tree[slider_id].set_local_position(Vector2::create(100.0, 50.0));
/// node_tree[slider_id].set_content_size(Vector2::create(200.0, 20.0));
/// let volume = Rc::new(Cell::new(0.0));
/// let volume_for_event = volume.clone();
/// let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
/// slider.set_range(0.0, 100.0);
/// slider.set_track_thickness(6.0);
/// slider.set_thumb_radius(10.0);
/// slider.set_value_changed_event(Box::new(move |slider: &mut UISlider, _context: &mut ComponentContext| {
/// 	volume_for_event.set(slider.get_value());
/// }));
///
/// node_tree.dispatch_touch(slider_id, &engine, TouchKind::Press, Vector2::create(150.0, 60.0));
/// assert_eq!(volume.get(), 25.0);
/// node_tree.dispatch_touch(slider_id, &engine, TouchKind::Moved, Vector2::create(500.0, 60.0));
/// assert_eq!(volume.get(), 100.0);
/// node_tree.dispatch_touch(slider_id, &engine, TouchKind::Release, Vector2::create(500.0, 60.0));
/// assert!(!node_tree[slider_id].get_component::<UISlider>().expect("slider").is_dragging());
/// ```
//==============================================================================
pub struct UISlider {
	ui_control: UIControl,
	value: f64,
	min_value: f64,
	max_value: f64,
	track_color: Color,
	fill_color: Color,
	thumb_color: Color,
	thumb_radius: f64,
	track_thickness: f64,
	thumb_node_id: Option<NodeId>,
	corner_radius: f64,
	direction: ProgressDirection,
	is_dragging: bool,
	is_interactable: bool,
	value_changed_event: Option<Rc<RefCell<UISliderEvent>>>,
	is_value_changed_fire_pending: bool,
}

#[crate::reflect(base = ui_control)]
impl UISlider {
	//==============================================================================
	/// 생성. (값 0, 범위 0 ~ 1, 트랙 회색, 채움 파랑, 손잡이 흰 원 반지름 16)
	/// - 트리에 붙이려면 node_tree.add_component(node_id, UISlider::new()) 를 쓴다.
	///
	/// # 돌려줌
	/// 가로 방향의 새 슬라이더. (트랙 두께 0, 둥근 모서리 0, 누를 수 있음)
	//==============================================================================
	pub fn new() -> UISlider {
		UISlider {
			ui_control: UIControl::new(),
			value: 0.0,
			min_value: 0.0,
			max_value: 1.0,
			track_color: Color::create(0.7, 0.7, 0.7, 1.0),
			fill_color: Color::create(0.23, 0.51, 0.96, 1.0),
			thumb_color: Color::white(),
			thumb_radius: 16.0,
			track_thickness: 0.0,
			thumb_node_id: None,
			corner_radius: 0.0,
			direction: ProgressDirection::Horizontal,
			is_dragging: false,
			is_interactable: true,
			value_changed_event: None,
			is_value_changed_fire_pending: false,
		}
	}

	//==============================================================================
	/// 품은 컨트롤 반환.
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uicontrol(&self) -> &UIControl {
		&self.ui_control
	}

	//==============================================================================
	/// 품은 컨트롤 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 UIControl.
	//==============================================================================
	pub fn get_uicontrol_mut(&mut self) -> &mut UIControl {
		&mut self.ui_control
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	///
	/// # 돌려줌
	/// 호스트 노드 밑에 만든 컨텐트 노드. 트리에 붙기 전이면 None.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_control.get_content()
	}

	//==============================================================================
	/// 손잡이 중심 자리. (호스트 영역 안)
	//==============================================================================
	fn calculate_thumb_center(&self, content_size: Vector2) -> Vector2 {
		let ratio = self.get_ratio();
		match self.direction {
			ProgressDirection::Vertical => {
				Vector2::create(content_size.x * 0.5, content_size.y * (1.0 - ratio))
			}
			ProgressDirection::Horizontal => {
				Vector2::create(content_size.x * ratio, content_size.y * 0.5)
			}
		}
	}

	//==============================================================================
	/// 사각형 채우기. (둥근 크기가 있으면 둥글게)
	//==============================================================================
	fn fill_rect(&self, graphic: &mut Graphic, rect: Rect, color: Color) {
		graphic.set_fill_color(color);
		if self.corner_radius > 0.0 {
			graphic.draw_round_rect(rect, self.corner_radius);
		}
		else {
			graphic.draw_rect(rect);
		}
	}

	//==============================================================================
	/// 값 바뀜 이벤트 부르기. (Rc 를 하나 더 쥐고 부르므로 콜백이 이벤트를 새로 정해도 된다. 이미 부르는 중이면 건너뛴다)
	//==============================================================================
	fn fire_value_changed_event(&mut self, context: &mut ComponentContext) {
		let value_changed_event = match self.get_value_changed_event() {
			Some(value_changed_event) => {
				value_changed_event
			}
			None => {
				return;
			}
		};
		let mut event_function = match value_changed_event.try_borrow_mut() {
			Ok(event_function) => {
				event_function
			}
			Err(_) => {
				return;
			}
		};
		(*event_function)(self, context);
	}

	//==============================================================================
	/// 입력 좌표로 값 갱신. (바뀌었으면 이벤트)
	//==============================================================================
	fn update_value_from_input(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let node_id = context.get_node_id();
		let local_position = match context.get_node_tree().convert_view_position_to_node_position(node_id, view_input_position) {
			Some(local_position) => {
				local_position
			}
			None => {
				return;
			}
		};
		let content_size = context.get_transform().get_content_size();
		let ratio = match self.direction {
			ProgressDirection::Vertical => {
				if content_size.y > 0.0 {
					1.0 - local_position.y / content_size.y
				}
				else {
					0.0
				}
			}
			ProgressDirection::Horizontal => {
				if content_size.x > 0.0 {
					local_position.x / content_size.x
				}
				else {
					0.0
				}
			}
		};
		let clamped_ratio = math::clamp(ratio, 0.0, 1.0);
		let previous_value = self.value;
		let new_value = self.min_value + (self.max_value - self.min_value) * clamped_ratio;
		self.set_value(new_value);
		if self.value != previous_value {
			self.fire_value_changed_event(context);
		}
	}

	//==============================================================================
	/// 0 ~ 1 비율 반환.
	///
	/// # 돌려줌
	/// 값이 범위에서 차지하는 비율. (0 ~ 1, 최대값이 최소값보다 크지 않으면 0)
	//==============================================================================
	pub fn get_ratio(&self) -> f64 {
		let range = self.max_value - self.min_value;
		if range <= 0.0 {
			return 0.0;
		}
		let ratio = (self.value - self.min_value) / range;
		math::clamp(ratio, 0.0, 1.0)
	}

	//==============================================================================
	/// 값 설정. (범위로 자른다. 이벤트는 부르지 않는다)
	///
	/// # 인자
	/// - `value`: 새 값. (최소값보다 작으면 최소값, 최대값보다 크면 최대값)
	//==============================================================================
	pub fn set_value(&mut self, value: f64) {
		self.value = math::clamp(value, self.min_value, self.max_value);
	}

	//==============================================================================
	/// 값 반환.
	///
	/// # 돌려줌
	/// 지금 값. (최소값 ~ 최대값)
	//==============================================================================
	pub fn get_value(&self) -> f64 {
		self.value
	}

	//==============================================================================
	/// 범위 설정. (지금 값을 새 범위로 다시 자른다)
	///
	/// # 인자
	/// - `min_value`: 최소값.
	/// - `max_value`: 최대값. (최소값보다 커야 한다, 같거나 작으면 비율은 늘 0)
	///
	/// # 예제
	/// 범위를 바꾸면 값도 다시 잘린다
	/// ```rust
	/// use vanilla::UISlider;
	///
	/// let mut slider = UISlider::new();
	/// slider.set_range(10.0, 20.0);
	/// assert_eq!(slider.get_value(), 10.0);
	/// slider.set_value(17.5);
	/// assert_eq!(slider.get_ratio(), 0.75);
	/// slider.set_value(30.0);
	/// assert_eq!(slider.get_value(), 20.0);
	/// slider.set_range(0.0, 5.0);
	/// assert_eq!(slider.get_value(), 5.0);
	/// ```
	//==============================================================================
	pub fn set_range(&mut self, min_value: f64, max_value: f64) {
		self.min_value = min_value;
		self.max_value = max_value;
		let value = self.value;
		self.set_value(value);
	}

	//==============================================================================
	/// 최소값 반환.
	///
	/// # 돌려줌
	/// 최소값. (기본 0)
	//==============================================================================
	pub fn get_min_value(&self) -> f64 {
		self.min_value
	}

	//==============================================================================
	/// 최대값 반환.
	///
	/// # 돌려줌
	/// 최대값. (기본 1)
	//==============================================================================
	pub fn get_max_value(&self) -> f64 {
		self.max_value
	}

	//==============================================================================
	/// 값 바뀜 이벤트를 억지로 부르기. (씬 코드에서 set_value 한 뒤. 다음 tick 에 부른다)
	///
	/// # 예제
	/// 값을 코드로 바꾸고 이벤트 알리기
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, UISlider};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let slider_id = node_tree.create_child(root_id, "Slider");
	/// let changed_count = Rc::new(Cell::new(0));
	/// let changed_count_for_event = changed_count.clone();
	/// let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
	/// slider.set_value_changed_event(Box::new(move |_slider: &mut UISlider, _context: &mut ComponentContext| {
	/// 	changed_count_for_event.set(changed_count_for_event.get() + 1);
	/// }));
	/// slider.set_value(0.5);
	/// slider.fire_value_changed();
	/// assert_eq!(changed_count.get(), 0);
	///
	/// node_tree.tick(&engine, 0.016);
	/// assert_eq!(changed_count.get(), 1);
	/// ```
	//==============================================================================
	pub fn fire_value_changed(&mut self) {
		self.is_value_changed_fire_pending = true;
	}

	//==============================================================================
	/// 트랙 색 설정.
	///
	/// # 인자
	/// - `color`: 트랙 색. (기본 회색 0.7)
	//==============================================================================
	pub fn set_track_color(&mut self, color: impl Into<Color>) {
		self.track_color = color.into();
	}

	//==============================================================================
	/// 트랙 색 반환.
	///
	/// # 돌려줌
	/// 트랙 색.
	//==============================================================================
	pub fn get_track_color(&self) -> Color {
		self.track_color
	}

	//==============================================================================
	/// 채움 색 설정.
	///
	/// # 인자
	/// - `color`: 최소값 쪽 끝부터 지금 값까지 채우는 색. (기본 파랑)
	//==============================================================================
	pub fn set_fill_color(&mut self, color: impl Into<Color>) {
		self.fill_color = color.into();
	}

	//==============================================================================
	/// 채움 색 반환.
	///
	/// # 돌려줌
	/// 채움 색.
	//==============================================================================
	pub fn get_fill_color(&self) -> Color {
		self.fill_color
	}

	//==============================================================================
	/// 손잡이 색 설정.
	///
	/// # 인자
	/// - `color`: 손잡이 원의 색. (기본 흰색, 손잡이 노드를 정하면 원을 그리지 않는다)
	//==============================================================================
	pub fn set_thumb_color(&mut self, color: impl Into<Color>) {
		self.thumb_color = color.into();
	}

	//==============================================================================
	/// 손잡이 색 반환.
	///
	/// # 돌려줌
	/// 손잡이 색.
	//==============================================================================
	pub fn get_thumb_color(&self) -> Color {
		self.thumb_color
	}

	//==============================================================================
	/// 손잡이 반지름 설정.
	///
	/// # 인자
	/// - `thumb_radius`: 손잡이 원의 반지름. (호스트 content_size 와 같은 단위, 기본 16)
	//==============================================================================
	pub fn set_thumb_radius(&mut self, thumb_radius: f64) {
		Component::mark_drawn_rect_dirty(self);
		self.thumb_radius = thumb_radius;
	}

	//==============================================================================
	/// 손잡이 반지름 반환.
	///
	/// # 돌려줌
	/// 손잡이 원의 반지름.
	//==============================================================================
	pub fn get_thumb_radius(&self) -> f64 {
		self.thumb_radius
	}

	//==============================================================================
	/// 트랙 두께 설정. (0 이면 호스트 영역 전체, 아니면 가운데에 그 두께로)
	///
	/// # 인자
	/// - `track_thickness`: 트랙 두께. (호스트 content_size 와 같은 단위, 기본 0)
	//==============================================================================
	pub fn set_track_thickness(&mut self, track_thickness: f64) {
		self.track_thickness = track_thickness;
	}

	//==============================================================================
	/// 트랙 두께 반환.
	///
	/// # 돌려줌
	/// 트랙 두께. (0 이면 호스트 영역 전체)
	//==============================================================================
	pub fn get_track_thickness(&self) -> f64 {
		self.track_thickness
	}

	//==============================================================================
	/// 손잡이 노드 설정. (호스트의 자식. 정하면 원 대신 이 노드를 손잡이 자리로 옮긴다)
	/// - tick 에서 손잡이 노드의 로컬 위치를 (손잡이 중심 - 노드 크기 절반) 에 둔다. (피봇이 왼쪽 위면 가운데가 손잡이 자리에 온다)
	///
	/// # 인자
	/// - `thumb_node_id`: 손잡이로 쓸 노드. (None 이면 원을 그린다)
	///
	/// # 예제
	/// 그림 노드를 손잡이로 쓰기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, Pivot, UISlider, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let slider_id = node_tree.create_child(root_id, "Slider");
	/// node_tree[slider_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[slider_id].set_content_size(Vector2::create(200.0, 20.0));
	/// node_tree.add_component(slider_id, UISlider::new());
	/// let thumb_id = node_tree.create_child(slider_id, "Thumb");
	/// node_tree[thumb_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[thumb_id].set_content_size(Vector2::create(10.0, 10.0));
	/// let slider = node_tree[slider_id].get_component_mut::<UISlider>().expect("slider");
	/// slider.set_thumb_node(Some(thumb_id));
	/// slider.set_value(0.5);
	///
	/// node_tree.tick(&engine, 0.016);
	/// assert_eq!(node_tree[thumb_id].get_local_position(), Vector2::create(95.0, 5.0));
	/// ```
	//==============================================================================
	pub fn set_thumb_node(&mut self, thumb_node_id: Option<NodeId>) {
		Component::mark_drawn_rect_dirty(self);
		self.thumb_node_id = thumb_node_id;
	}

	//==============================================================================
	/// 손잡이 노드 반환.
	///
	/// # 돌려줌
	/// 손잡이 노드. 정하지 않았으면 None.
	//==============================================================================
	pub fn get_thumb_node(&self) -> Option<NodeId> {
		self.thumb_node_id
	}

	//==============================================================================
	/// 둥근 모서리 반지름 설정.
	///
	/// # 인자
	/// - `corner_radius`: 트랙과 채움 사각형의 둥근 모서리 반지름. (0 이면 각진 사각형, 기본 0)
	//==============================================================================
	pub fn set_corner_radius(&mut self, corner_radius: f64) {
		self.corner_radius = corner_radius;
	}

	//==============================================================================
	/// 둥근 모서리 반지름 반환.
	///
	/// # 돌려줌
	/// 둥근 모서리 반지름.
	//==============================================================================
	pub fn get_corner_radius(&self) -> f64 {
		self.corner_radius
	}

	//==============================================================================
	/// 방향 설정.
	///
	/// # 인자
	/// - `direction`: 방향. (Horizontal 은 왼쪽이 최소값, Vertical 은 아래가 최소값, 기본 Horizontal)
	//==============================================================================
	pub fn set_direction(&mut self, direction: ProgressDirection) {
		Component::mark_drawn_rect_dirty(self);
		self.direction = direction;
	}

	//==============================================================================
	/// 방향 반환.
	///
	/// # 돌려줌
	/// 지금 방향.
	//==============================================================================
	pub fn get_direction(&self) -> ProgressDirection {
		self.direction
	}

	//==============================================================================
	/// 값 바뀜 이벤트 설정.
	///
	/// # 인자
	/// - `value_changed_event`: 누르거나 끌어 값이 바뀔 때 부를 함수. (슬라이더와 문맥을 받는다, 같은 값이면 불리지 않고 set_value 로 바꿀 때도 불리지 않는다)
	//==============================================================================
	pub fn set_value_changed_event(&mut self, value_changed_event: UISliderEvent) {
		self.value_changed_event = Some(Rc::new(RefCell::new(value_changed_event)));
	}

	//==============================================================================
	/// 값 바뀜 이벤트 반환. (부를 때는 borrow_mut 로 빌린다)
	///
	/// # 돌려줌
	/// 건 값 바뀜 이벤트. 걸지 않았으면 None.
	//==============================================================================
	pub fn get_value_changed_event(&self) -> Option<Rc<RefCell<UISliderEvent>>> {
		self.value_changed_event.clone()
	}

	//==============================================================================
	/// 사용 가능 설정. (끄면 누름을 받지 않는다)
	///
	/// # 인자
	/// - `is_interactable`: 누름을 받으려면 true. (기본 true, 끄더라도 이미 끄는 중인 손가락은 끝까지 따라간다)
	//==============================================================================
	pub fn set_interactable(&mut self, is_interactable: bool) {
		self.is_interactable = is_interactable;
	}

	//==============================================================================
	/// 사용 가능 여부.
	///
	/// # 돌려줌
	/// 누름을 받으면 true.
	//==============================================================================
	pub fn get_interactable(&self) -> bool {
		self.is_interactable
	}

	//==============================================================================
	/// 끄는 중인지.
	///
	/// # 돌려줌
	/// 누름부터 뗌이나 취소까지 true.
	//==============================================================================
	pub fn is_dragging(&self) -> bool {
		self.is_dragging
	}
}

impl Default for UISlider {
	//==============================================================================
	/// 기본값. (UISlider::new 와 같다)
	//==============================================================================
	fn default() -> UISlider {
		UISlider::new()
	}
}

#[crate::reflect]
impl Component for UISlider {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 UIControl 자리, get_component::<UIControl>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.ui_control)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.ui_control)
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (품은 컨트롤의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_control.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_control.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("UISlider")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UISlider"
	}

	//==============================================================================
	/// 트리에 붙을 때. (컨텐트 노드를 만들고 호스트 노드를 interactable 로 켠다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_control.get_uiview_mut().create_content_node(node_tree, node_id);
		node_tree[node_id].set_interactable(true);
	}

	//==============================================================================
	/// tick 이 필요한지. (손잡이 노드가 있거나 미룬 값 바뀜 이벤트가 있을 때만)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		self.thumb_node_id.is_some() || self.is_value_changed_fire_pending
	}

	//==============================================================================
	/// 내용 영역 안에서만 그리는지. (false, 트랙과 손잡이를 내용 크기 밖까지 그린다)
	/// - 원본 UIView.isDrawnInsideContent 는 draw 를 덮어쓴 UISlider 에서 false 다. 인스펙터에 같은 자리로 보인다.
	//==============================================================================
	#[reflect(after = "isDragging")]
	fn is_drawn_inside_content(&self) -> bool {
		false
	}

	//==============================================================================
	/// 그리는 사각형. (원본 UISlider.calculateDrawnRect)
	/// - 트랙과 채움은 내용 영역, 손잡이 원은 값이 움직이는 줄 전체에 반지름만큼. (값과 상관없는 사각형이라 값을 바꿔도 다시 셀 일이 없다)
	/// - 손잡이 노드를 정했으면 내용 영역만. (손잡이 노드는 저마다 그린다) 한 단위씩 넓힌다.
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, _graphic: &mut Graphic) -> Option<Rect> {
		let content_size = transform.get_content_size();
		let mut minimum_x = 0.0_f64;
		let mut minimum_y = 0.0_f64;
		let mut maximum_x = content_size.x;
		let mut maximum_y = content_size.y;
		if self.thumb_node_id.is_none() && self.thumb_radius > 0.0 {
			let thumb_radius = self.thumb_radius;
			if self.direction == ProgressDirection::Vertical {
				minimum_x = minimum_x.min(content_size.x * 0.5 - thumb_radius);
				maximum_x = maximum_x.max(content_size.x * 0.5 + thumb_radius);
				minimum_y = minimum_y.min(-thumb_radius);
				maximum_y = maximum_y.max(content_size.y + thumb_radius);
			}
			else {
				minimum_x = minimum_x.min(-thumb_radius);
				maximum_x = maximum_x.max(content_size.x + thumb_radius);
				minimum_y = minimum_y.min(content_size.y * 0.5 - thumb_radius);
				maximum_y = maximum_y.max(content_size.y * 0.5 + thumb_radius);
			}
		}
		Some(Rect::create(minimum_x - 1.0, minimum_y - 1.0, maximum_x - minimum_x + 2.0, maximum_y - minimum_y + 2.0))
	}

	//==============================================================================
	/// 갱신. (미뤄 둔 값 바뀜 이벤트, 손잡이 노드 자리)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
		if self.is_value_changed_fire_pending {
			self.is_value_changed_fire_pending = false;
			self.fire_value_changed_event(context);
		}
		let thumb_node_id = match self.thumb_node_id {
			Some(thumb_node_id) => {
				thumb_node_id
			}
			None => {
				return;
			}
		};
		let content_size = context.get_transform().get_content_size();
		let thumb_center = self.calculate_thumb_center(content_size);
		if let Some(thumb_node) = context.get_node_tree_mut().get_node_mut(thumb_node_id) {
			// 원본처럼 로컬 위치를 (중심 - 크기 절반) 에 둔다. (피봇이 왼쪽 위인 손잡이 노드면 가운데가 손잡이 자리에 온다)
			let thumb_size = thumb_node.get_content_size();
			let thumb_position = Vector2::create(thumb_center.x - thumb_size.x * 0.5, thumb_center.y - thumb_size.y * 0.5);
			thumb_node.set_local_position(thumb_position);
		}
	}

	//==============================================================================
	/// 출력. (트랙 + 채움 + 손잡이)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}

		// 트랙. (두께를 정했으면 노드 가운데에 그 두께로 그린다)
		let track_thickness = self.track_thickness;
		let mut track_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		if track_thickness > 0.0 {
			track_rect = match self.direction {
				ProgressDirection::Vertical => {
					Rect::create((content_size.x - track_thickness) * 0.5, 0.0, track_thickness, content_size.y)
				}
				ProgressDirection::Horizontal => {
					Rect::create(0.0, (content_size.y - track_thickness) * 0.5, content_size.x, track_thickness)
				}
			};
		}
		let track_color = self.track_color;
		self.fill_rect(graphic, track_rect, track_color);

		// 채움.
		let ratio = self.get_ratio();
		if ratio > 0.0 {
			let fill_rect = match self.direction {
				ProgressDirection::Vertical => {
					let fill_height = content_size.y * ratio;
					Rect::create(track_rect.position.x, content_size.y - fill_height, track_rect.size.x, fill_height)
				}
				ProgressDirection::Horizontal => {
					let fill_width = content_size.x * ratio;
					Rect::create(0.0, track_rect.position.y, fill_width, track_rect.size.y)
				}
			};
			let fill_color = self.fill_color;
			self.fill_rect(graphic, fill_rect, fill_color);
		}

		// 손잡이 노드를 정했으면 원은 그리지 않는다. (자리는 tick 이 옮긴다)
		if self.thumb_node_id.is_some() {
			return;
		}
		let thumb_center = self.calculate_thumb_center(content_size);
		graphic.set_fill_color(self.thumb_color);
		graphic.draw_circle(thumb_center, self.thumb_radius);
	}

	//==============================================================================
	/// 터치 누름. (누를 수 있으면 끌기를 시작하고 그 자리로 값을 바꾼다)
	//==============================================================================
	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_interactable {
			return;
		}
		self.is_dragging = true;
		self.update_value_from_input(context, view_input_position);
	}

	//==============================================================================
	/// 터치 이동. (끄는 중이면 그 자리로 값을 바꾼다)
	//==============================================================================
	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_dragging {
			return;
		}
		self.update_value_from_input(context, view_input_position);
	}

	//==============================================================================
	/// 터치 뗌. (끄는 중이면 그 자리로 값을 바꾸고 끌기를 끝낸다)
	//==============================================================================
	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_dragging {
			return;
		}
		self.update_value_from_input(context, view_input_position);
		self.is_dragging = false;
	}

	//==============================================================================
	/// 터치 취소. (값은 그대로 두고 끌기를 끝낸다)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.is_dragging = false;
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
	/// 값은 범위로 잘린다. 범위를 바꾸면 지금 값도 다시 잘린다.
	//==============================================================================
	#[test]
	fn value_clamp() {
		let mut slider = UISlider::new();
		slider.set_value(2.0);
		assert_eq!(slider.get_value(), 1.0);
		slider.set_value(-1.0);
		assert_eq!(slider.get_value(), 0.0);
		slider.set_range(10.0, 20.0);
		assert_eq!(slider.get_value(), 10.0);
		slider.set_value(17.5);
		assert_eq!(slider.get_ratio(), 0.75);
		slider.set_range(0.0, 5.0);
		assert_eq!(slider.get_value(), 5.0);
	}

	//==============================================================================
	/// 누르고 끌면 그 자리 비율로 값이 바뀌고 이벤트가 난다. 영역 밖으로 끌면 끝 값에 멈춘다.
	//==============================================================================
	#[test]
	fn drag_updates_value() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let slider_id = node_tree.create_child(root_id, "slider");
		let slider_node = &mut node_tree[slider_id];
		slider_node.set_pivot(Pivot::TOP_LEFT);
		slider_node.set_local_position(Vector2::create(100.0, 50.0));
		slider_node.set_content_size(Vector2::create(200.0, 20.0));
		let changed_count = Rc::new(Cell::new(0));
		{
			let slider = node_tree.add_component(slider_id, UISlider::new()).expect("slider");
			slider.set_range(0.0, 100.0);
			let changed_count_for_event = changed_count.clone();
			slider.set_value_changed_event(Box::new(move |_slider: &mut UISlider, _context: &mut ComponentContext| {
				changed_count_for_event.set(changed_count_for_event.get() + 1);
			}));
			assert!(slider.get_value_changed_event().is_some());
		}
		assert!(node_tree[slider_id].is_interactable());

		node_tree.dispatch_touch(slider_id, &engine, TouchKind::Press, Vector2::create(150.0, 60.0));
		assert_eq!(node_tree[slider_id].get_component::<UISlider>().expect("slider").get_value(), 25.0);
		assert_eq!(changed_count.get(), 1);
		node_tree.dispatch_touch(slider_id, &engine, TouchKind::Moved, Vector2::create(500.0, 60.0));
		assert_eq!(node_tree[slider_id].get_component::<UISlider>().expect("slider").get_value(), 100.0);
		assert_eq!(changed_count.get(), 2);
		// 같은 값이면 이벤트가 나지 않는다.
		node_tree.dispatch_touch(slider_id, &engine, TouchKind::Moved, Vector2::create(600.0, 60.0));
		assert_eq!(changed_count.get(), 2);
		node_tree.dispatch_touch(slider_id, &engine, TouchKind::Release, Vector2::create(100.0, 60.0));
		assert_eq!(node_tree[slider_id].get_component::<UISlider>().expect("slider").get_value(), 0.0);
		assert_eq!(changed_count.get(), 3);
		assert!(!node_tree[slider_id].get_component::<UISlider>().expect("slider").is_dragging());

		// 손잡이 노드는 tick 에서 손잡이 자리로 옮겨진다.
		let thumb_id = node_tree.create_child(slider_id, "thumb");
		node_tree[thumb_id].set_pivot(Pivot::TOP_LEFT);
		node_tree[thumb_id].set_content_size(Vector2::create(10.0, 10.0));
		{
			let slider = node_tree[slider_id].get_component_mut::<UISlider>().expect("slider");
			slider.set_thumb_node(Some(thumb_id));
			slider.set_value(50.0);
			slider.fire_value_changed();
		}
		assert_eq!(changed_count.get(), 3);
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[thumb_id].get_local_position(), Vector2::create(95.0, 5.0));
		assert_eq!(changed_count.get(), 4);
	}
}
