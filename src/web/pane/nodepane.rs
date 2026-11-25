//==============================================================================
// 엔진 노드 화면 분할 패널. (원본 web/pane.js 의 PaneStyle, Pane 을 데스크탑에서 노드로 짓는 판, 공통 형과 셈은 web/pane.rs)
// - 원본의 DOM 요소 자리가 노드다. 컨테이너는 노드(get_container)이고, 내용은 node_tree.add_child 로 그 자식에 붙인다. (원본 appendChild)
//   데스크탑에는 전역 document 가 없어 노드를 만들고 고치는 함수는 NodeTree 를 받는다. 길이 단위는 원본의 CSS 픽셀 자리인 노드 좌표다.
//     let main_pane = Pane::new(node_tree, PaneOptions { direction: PaneDirection::Horizontal, ..Default::default() });
//     let left_pane = Pane::new(node_tree, PaneOptions { size: PaneSize::Fixed(240.0), min_size: 170.0, ..Default::default() });
//     main_pane.add_pane(node_tree, &left_pane);
//     let panel_id = PaneStyle::create(node_tree, "div", "panel", &PaneStyleOptions::default());
//     node_tree.add_child(left_pane.get_container(), panel_id);
//     main_pane.attach_to(node_tree, root_id);
// - 컨테이너는 원본처럼 바탕을 COLOR_BACKGROUND 로 칠하고 넘치는 자식을 자른다. (overflow: hidden, Mask)
// - 리사이저는 원본처럼 끌 수 있으면 두께 5, 아니면 1 의 선이고 COLOR_RESIZER 로, 올려 두거나 끄는 동안 COLOR_RESIZER_HOVER 로 칠한다.
//   눌러 끌면 잡은 자리와 경계 사이 간격을 지켜 앞이나 뒤 자식의 고정 크기를 바꾸고 뿌리부터 다시 놓는다. 올려 두거나 끄는 동안 col-resize, row-resize 커서다.
// - attach_to 한 뿌리 패널은 부모 노드 크기가 바뀌면 다시 놓는다. (원본 창 resize 리스너와 다음 프레임의 refresh)
// - PaneStyle::create 는 원본 프리셋과 같은 모양의 노드를 짓는다. (PaneElement 컴포넌트)
//   공통: 왼쪽 위에 놓고 부모 크기를 채운다(position absolute, 100%), 글꼴 13px sans-serif, 글 색 COLOR_TEXT.
//   panel: 바탕 COLOR_PANEL, 안쪽 여백 10. toolbar: 바탕 COLOR_TOOLBAR, 아래 테두리 1px COLOR_BORDER, 가로로 가운데 모아 10 씩 띄운다(flex).
//   button: 제 크기(position relative, auto), 안쪽 여백 6 16, 모서리 4, 굵은 글, 바탕 COLOR_RESIZER(accent 면 COLOR_ACCENT, warning 이면 COLOR_WARNING),
//   올려 두면 밝기 1.2 배와 pointer 커서, 누르고 떼면 set_on_click 의 함수. input: 엔진 입력 필드(UIInputField), 바탕 COLOR_INPUT_BACKGROUND, 안쪽 여백 4.
//   textarea: 바탕 COLOR_BACKGROUND, 글 #d4d4d4, 고정폭 14px, 안쪽 여백 15. text: 제 크기, 굵은 글.
//   relative 요소는 부모 안쪽 여백 안에서 차례로 놓는다. (부모가 flex 면 가로로, 아니면 세로로. div 는 폭을 채우고 그 밖은 제 폭)
// - 원본과 다른 점:
//   - 원본 요소에 addEventListener("click") 으로 다는 것은 PaneElement::set_on_click 이다.
//   - options.style 과 PaneStyle::apply 가 읽는 CSS 는 프리셋이 쓰는 것과 그 둘레다: position(absolute, relative), left, top, width, height(px, %, auto), display(flex, block, none),
//     justify-content(center), align-items(center), gap, padding(1 ~ 4 값), margin-bottom, background, background-color, color, font-size, font-weight, font-family,
//     border, border-bottom(굵기와 색), border-radius, cursor(pointer). 그 밖의 CSS(flex 비율, 세로 flex, overflow 스크롤 같은 것)는 읽지 않는다.
//   - textarea 프리셋은 글을 보이기만 한다. 엔진에 여러 줄 입력 필드가 없다.
//   - canvas 태그는 아무것도 그리지 않는 노드다. 원본은 그 자리에 엔진 캔버스가 들어가고, 데스크탑은 창 전체가 엔진 화면이라 게임이 그 노드의 자리를 읽어 쓴다.
//==============================================================================
use super::{calculate_pane_layout, calculate_resized_size, calculate_resizer_thickness, PaneDirection, PaneLayoutChild, PaneOptions, PaneResizableEdges, PaneResizeCallback, PaneSize, PaneStyleOptions, PaneTheme};
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{Graphic, TextAlign, TextBaseline};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textstringtexturecache::measure_text_without_graphic;
use crate::ui::uiinputfield::UIInputField;
use crate::ui::uiview::is_view_position_inside_node;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

// 단추를 누르고 떼면 부르는 함수. (원본 click 리스너)
pub type PaneClickCallback = Box<dyn FnMut(&mut ComponentContext)>;

//==============================================================================
// 자식 사이 리사이저 하나.
//==============================================================================
#[derive(Clone, Copy)]
struct PaneResizer {
	node_id: NodeId,
	thickness: f64,
}

//==============================================================================
// 패널 상태. (Pane 손잡이들과 컨테이너, 리사이저 노드의 컴포넌트가 나눠 든다)
//==============================================================================
struct PaneState {
	container_id: NodeId,
	direction: PaneDirection,
	children: Vec<Pane>,
	resizer_list: Vec<PaneResizer>,
	initial_size: PaneSize,
	fixed_size: f64,
	min_size: f64,
	max_size: f64,
	is_resizable: bool,
	resizable_edges: PaneResizableEdges,
	is_visible: bool,
	resize_callback: Option<PaneResizeCallback>,
	parent_pane: Weak<RefCell<PaneState>>,
	// 마지막으로 놓인 자리. ([x, y, 폭, 높이], 부모 컨테이너 기준)
	layout_rect: [f64; 4],
	// attach_to 한 뿌리인지와 그때 부모 크기. (바뀌면 다시 놓는다)
	is_attached: bool,
	attached_parent_size: Option<Vector2>,
	// 리사이저 끌기.
	active_resizer_index: Option<usize>,
	resize_grab_offset: f64,
}

//==============================================================================
// 화면 분할 패널. (원본 Pane, clone 은 같은 패널을 가리킨다)
//==============================================================================
#[derive(Clone)]
pub struct Pane {
	state: Rc<RefCell<PaneState>>,
}

impl Pane {
	//==============================================================================
	// 생성. (떨어진 컨테이너 노드를 만든다. 바탕과 자르기)
	//==============================================================================
	pub fn new(node_tree: &mut NodeTree, options: PaneOptions) -> Pane {
		let fixed_size = match options.size {
			PaneSize::Fixed(pixel_size) => {
				pixel_size
			}
			PaneSize::Flex | PaneSize::Ratio(_) => {
				0.0
			}
		};
		let container_id = node_tree.create_node("paneContainer");
		{
			let container_node = &mut node_tree[container_id];
			container_node.set_pivot(Pivot::TOP_LEFT);
			container_node.set_anchor(Vector2::zero());
		}
		let state = Rc::new(RefCell::new(PaneState {
			container_id,
			direction: options.direction,
			children: Vec::new(),
			resizer_list: Vec::new(),
			initial_size: options.size,
			fixed_size,
			min_size: options.min_size,
			max_size: options.max_size,
			is_resizable: options.is_resizable,
			resizable_edges: options.resizable_edges,
			is_visible: true,
			resize_callback: None,
			parent_pane: Weak::new(),
			layout_rect: [0.0; 4],
			is_attached: false,
			attached_parent_size: None,
			active_resizer_index: None,
			resize_grab_offset: 0.0,
		}));
		node_tree.add_component(container_id, PaneContainer {
			component_base: ComponentBase::new(),
			state: Rc::downgrade(&state),
		});
		node_tree.add_component(container_id, Mask::new());
		Pane { state }
	}

	//==============================================================================
	// 자식 추가. (앞 자식이 있으면 그 사이에 리사이저 노드를 둔다)
	//==============================================================================
	pub fn add_pane(&self, node_tree: &mut NodeTree, pane: &Pane) {
		pane.state.borrow_mut().parent_pane = Rc::downgrade(&self.state);
		let previous_pane = self.state.borrow().children.last().cloned();
		let container_id = self.get_container();
		if let Some(previous_pane) = previous_pane {
			let (direction, is_resizable, resizer_index) = {
				let state = self.state.borrow();
				(state.direction, state.is_resizable, state.resizer_list.len())
			};
			let (can_resize, thickness) = calculate_resizer_thickness(direction, is_resizable, previous_pane.is_resizable(), previous_pane.get_resizable_edges(), pane.is_resizable(), pane.get_resizable_edges());
			let resizer_id = node_tree.create_child(container_id, "paneResizer");
			{
				let resizer_node = &mut node_tree[resizer_id];
				resizer_node.set_pivot(Pivot::TOP_LEFT);
				resizer_node.set_anchor(Vector2::zero());
			}
			node_tree.add_component(resizer_id, PaneResizerHandle {
				component_base: ComponentBase::new(),
				state: Rc::downgrade(&self.state),
				resizer_index,
				can_resize,
				is_hovered: false,
			});
			self.state.borrow_mut().resizer_list.push(PaneResizer {
				node_id: resizer_id,
				thickness,
			});
		}
		self.state.borrow_mut().children.push(pane.clone());
		node_tree.add_child(container_id, pane.get_container());
		// 리사이저는 원본 z-index 1000 처럼 자식 패널 위에 둔다.
		let resizer_id_list: Vec<NodeId> = self.state.borrow().resizer_list.iter().map(|resizer| resizer.node_id).collect();
		for resizer_id in resizer_id_list {
			let child_count = node_tree.get_child_count(container_id);
			node_tree.set_sibling_index(resizer_id, child_count.saturating_sub(1));
		}
	}

	//==============================================================================
	// 배치. (자기 자리를 놓고, 같은 셈으로 자식과 리사이저를 놓는다. 원본 updateLayout)
	//==============================================================================
	pub fn update_layout(&self, node_tree: &mut NodeTree, x: f64, y: f64, width: f64, height: f64) {
		let (container_id, direction, children, resizer_list) = {
			let mut state = self.state.borrow_mut();
			state.layout_rect = [x, y, width, height];
			(state.container_id, state.direction, state.children.clone(), state.resizer_list.clone())
		};
		if let Some(container_node) = node_tree.get_node_mut(container_id) {
			container_node.set_local_position(Vector2::create(x as f32, y as f32));
			container_node.set_content_size(Vector2::create(width as f32, height as f32));
		}
		if children.is_empty() {
			self.fire_resize_callback();
			return;
		}
		let child_list: Vec<PaneLayoutChild> = children.iter().map(|child| PaneLayoutChild {
			is_visible: child.is_visible(),
			initial_size: child.get_initial_size(),
			fixed_size: child.get_fixed_size(),
			min_size: child.get_min_size(),
			max_size: child.get_max_size(),
		}).collect();
		let resizer_thickness_list: Vec<f64> = resizer_list.iter().map(|resizer| resizer.thickness).collect();
		let layout_result = calculate_pane_layout(direction, width, height, &child_list, &resizer_thickness_list);
		for (index, resizer) in resizer_list.iter().enumerate() {
			if let Some(resizer_node) = node_tree.get_node_mut(resizer.node_id) {
				match layout_result.resizer_rect_list[index] {
					Some(resizer_rect) => {
						resizer_node.set_active(true);
						resizer_node.set_local_position(Vector2::create(resizer_rect[0] as f32, resizer_rect[1] as f32));
						resizer_node.set_content_size(Vector2::create(resizer_rect[2] as f32, resizer_rect[3] as f32));
					}
					None => {
						resizer_node.set_active(false);
					}
				}
			}
		}
		for (index, child) in children.iter().enumerate() {
			let child_container_id = child.get_container();
			match layout_result.child_rect_list[index] {
				Some(child_rect) => {
					if let Some(child_node) = node_tree.get_node_mut(child_container_id) {
						child_node.set_active(true);
					}
					child.update_layout(node_tree, child_rect[0], child_rect[1], child_rect[2], child_rect[3]);
				}
				None => {
					if let Some(child_node) = node_tree.get_node_mut(child_container_id) {
						child_node.set_active(false);
					}
				}
			}
		}
		self.fire_resize_callback();
	}

	//==============================================================================
	// 배치 콜백 부르기. (콜백 안에서 이 패널을 다시 불러도 되게 빼 두었다가 돌려놓는다)
	//==============================================================================
	fn fire_resize_callback(&self) {
		let resize_callback = self.state.borrow_mut().resize_callback.take();
		if let Some(mut resize_callback) = resize_callback {
			resize_callback();
			let mut state = self.state.borrow_mut();
			if state.resize_callback.is_none() {
				state.resize_callback = Some(resize_callback);
			}
		}
	}

	//==============================================================================
	// 배치 초기화. (끌어서 바꾼 크기를 지운다, 자식까지)
	//==============================================================================
	pub fn reset_layout(&self) {
		let children = {
			let mut state = self.state.borrow_mut();
			state.fixed_size = 0.0;
			state.children.clone()
		};
		for child in children.iter() {
			child.reset_layout();
		}
	}

	//==============================================================================
	// 부모 노드 크기에 맞춰 다시 놓는다. (부모가 없으면 아무것도 하지 않는다)
	//==============================================================================
	pub fn refresh(&self, node_tree: &mut NodeTree) {
		let parent_id = match node_tree.get_parent_id(self.get_container()) {
			Some(parent_id) => {
				parent_id
			}
			None => {
				return;
			}
		};
		let parent_size = node_tree[parent_id].get_content_size();
		self.state.borrow_mut().attached_parent_size = Some(parent_size);
		self.update_layout(node_tree, 0.0, 0.0, parent_size.x as f64, parent_size.y as f64);
	}

	//==============================================================================
	// 노드에 단다. (그 자리에서 놓고, 뒤로 부모 크기가 바뀌면 다시 놓는다. 원본 attachTo 의 창 resize 리스너)
	//==============================================================================
	pub fn attach_to(&self, node_tree: &mut NodeTree, parent_id: NodeId) {
		node_tree.add_child(parent_id, self.get_container());
		self.state.borrow_mut().is_attached = true;
		self.refresh(node_tree);
	}

	//==============================================================================
	// 보이기 / 숨기기. (숨긴 패널은 자리를 차지하지 않고, 맞닿은 리사이저도 숨는다)
	//==============================================================================
	pub fn set_visible(&self, node_tree: &mut NodeTree, is_visible: bool) {
		let container_id = {
			let mut state = self.state.borrow_mut();
			state.is_visible = is_visible;
			state.container_id
		};
		if let Some(container_node) = node_tree.get_node_mut(container_id) {
			container_node.set_active(is_visible);
		}
		self.find_root_pane().refresh(node_tree);
	}

	//==============================================================================
	// 뿌리 패널. (부모를 따라 끝까지 올라간다)
	//==============================================================================
	fn find_root_pane(&self) -> Pane {
		let mut root_pane = self.clone();
		loop {
			let parent_state = root_pane.state.borrow().parent_pane.upgrade();
			match parent_state {
				Some(parent_state) => {
					root_pane = Pane { state: parent_state };
				}
				None => {
					return root_pane;
				}
			}
		}
	}

	//==============================================================================
	// 보이는지.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.state.borrow().is_visible
	}

	//==============================================================================
	// flex 인지. (처음 크기가 Flex)
	//==============================================================================
	pub fn is_flex(&self) -> bool {
		self.state.borrow().initial_size == PaneSize::Flex
	}

	//==============================================================================
	// 처음 크기.
	//==============================================================================
	pub fn get_initial_size(&self) -> PaneSize {
		self.state.borrow().initial_size
	}

	//==============================================================================
	// 고정 크기. (픽셀, 0 이면 처음 크기로 잰다)
	//==============================================================================
	pub fn get_fixed_size(&self) -> f64 {
		self.state.borrow().fixed_size
	}

	//==============================================================================
	// 고정 크기 설정. (다음 배치부터 쓴다. flex 패널에는 쓰이지 않는다)
	//==============================================================================
	pub fn set_fixed_size(&self, fixed_size: f64) {
		self.state.borrow_mut().fixed_size = fixed_size;
	}

	//==============================================================================
	// 최소 크기.
	//==============================================================================
	pub fn get_min_size(&self) -> f64 {
		self.state.borrow().min_size
	}

	//==============================================================================
	// 최대 크기.
	//==============================================================================
	pub fn get_max_size(&self) -> f64 {
		self.state.borrow().max_size
	}

	//==============================================================================
	// 컨테이너 노드. (내용을 여기의 자식으로 붙인다)
	//==============================================================================
	pub fn get_container(&self) -> NodeId {
		self.state.borrow().container_id
	}

	//==============================================================================
	// 배치 콜백 설정. (update_layout 이 끝날 때마다 부른다. None 이면 비운다)
	//==============================================================================
	pub fn set_callback(&self, resize_callback: Option<PaneResizeCallback>) {
		self.state.borrow_mut().resize_callback = resize_callback;
	}

	//==============================================================================
	// 리사이저로 끌 수 있는지.
	//==============================================================================
	pub fn is_resizable(&self) -> bool {
		self.state.borrow().is_resizable
	}

	//==============================================================================
	// 끌 수 있는 가장자리.
	//==============================================================================
	pub fn get_resizable_edges(&self) -> PaneResizableEdges {
		self.state.borrow().resizable_edges
	}

	//==============================================================================
	// 리사이저 끌기 시작. (잡은 자리와 경계 사이 간격을 기억해 두어야 처음 끌 때 크기가 튀지 않는다)
	//==============================================================================
	fn begin_resize(&self, resizer_index: usize, pointer: f64) {
		let mut state = self.state.borrow_mut();
		let direction = state.direction;
		let child_rect = match state.children.get(resizer_index + 1) {
			Some(next_pane) => {
				next_pane.state.borrow().layout_rect
			}
			None => {
				return;
			}
		};
		let thickness = state.resizer_list[resizer_index].thickness;
		let resizer_start = match direction {
			PaneDirection::Horizontal => {
				child_rect[0] - thickness
			}
			PaneDirection::Vertical => {
				child_rect[1] - thickness
			}
		};
		state.active_resizer_index = Some(resizer_index);
		state.resize_grab_offset = pointer - resizer_start;
	}

	//==============================================================================
	// 리사이저 끌기. (같은 셈으로 앞이나 뒤 자식의 고정 크기를 바꾸고 뿌리부터 다시 놓는다. 원본 onMouseMove)
	//==============================================================================
	fn move_resize(&self, node_tree: &mut NodeTree, pointer: f64) {
		let (direction, previous_pane, next_pane, thickness, grab_offset) = {
			let state = self.state.borrow();
			let resizer_index = match state.active_resizer_index {
				Some(resizer_index) => {
					resizer_index
				}
				None => {
					return;
				}
			};
			match (state.children.get(resizer_index), state.children.get(resizer_index + 1)) {
				(Some(previous_pane), Some(next_pane)) => {
					(state.direction, previous_pane.clone(), next_pane.clone(), state.resizer_list[resizer_index].thickness, state.resize_grab_offset)
				}
				_ => {
					return;
				}
			}
		};
		let previous_rect = previous_pane.state.borrow().layout_rect;
		let next_rect = next_pane.state.borrow().layout_rect;
		let (previous_start, next_end) = match direction {
			PaneDirection::Horizontal => {
				(previous_rect[0], next_rect[0] + next_rect[2])
			}
			PaneDirection::Vertical => {
				(previous_rect[1], next_rect[1] + next_rect[3])
			}
		};
		let (is_next_changed, new_fixed_size) = calculate_resized_size(previous_start, next_end, thickness, pointer, grab_offset, previous_pane.get_min_size(), next_pane.get_min_size(), previous_pane.is_flex(), next_pane.is_flex());
		if is_next_changed {
			next_pane.set_fixed_size(new_fixed_size);
		}
		else {
			previous_pane.set_fixed_size(new_fixed_size);
		}
		self.find_root_pane().refresh(node_tree);
	}

	//==============================================================================
	// 리사이저 끌기 끝.
	//==============================================================================
	fn end_resize(&self) {
		self.state.borrow_mut().active_resizer_index = None;
	}
}

//==============================================================================
// 컨테이너 노드의 컴포넌트. (바탕을 칠하고, attach_to 한 뿌리는 부모 크기가 바뀌면 다시 놓는다)
//==============================================================================
struct PaneContainer {
	component_base: ComponentBase,
	state: Weak<RefCell<PaneState>>,
}

impl Component for PaneContainer {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"PaneContainer"
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let state = match self.state.upgrade() {
			Some(state) => {
				state
			}
			None => {
				return;
			}
		};
		let (is_attached, attached_parent_size) = {
			let state_reference = state.borrow();
			(state_reference.is_attached, state_reference.attached_parent_size)
		};
		if !is_attached {
			return;
		}
		let node_id = context.get_node_id();
		let parent_size = match context.get_node_tree().get_parent_id(node_id) {
			Some(parent_id) => {
				context.get_node_tree()[parent_id].get_content_size()
			}
			None => {
				return;
			}
		};
		if attached_parent_size != Some(parent_size) {
			Pane { state }.refresh(context.get_node_tree_mut());
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		graphic.set_fill_color(PaneTheme::COLOR_BACKGROUND);
		graphic.draw_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y));
	}
}

//==============================================================================
// 리사이저 노드의 컴포넌트. (올려 두면 색과 커서, 눌러 끌면 크기 바꾸기)
//==============================================================================
struct PaneResizerHandle {
	component_base: ComponentBase,
	state: Weak<RefCell<PaneState>>,
	resizer_index: usize,
	can_resize: bool,
	is_hovered: bool,
}

impl PaneResizerHandle {
	//==============================================================================
	// 포인터의 부모 컨테이너 좌표. (나누는 방향 쪽)
	//==============================================================================
	fn read_pointer(&self, context: &ComponentContext, view_input_position: Vector2, direction: PaneDirection) -> Option<f64> {
		let node_id = context.get_node_id();
		let container_id = context.get_node_tree().get_parent_id(node_id)?;
		let local_position = context.get_node_tree().convert_view_position_to_node_position(container_id, view_input_position)?;
		Some(match direction {
			PaneDirection::Horizontal => {
				local_position.x as f64
			}
			PaneDirection::Vertical => {
				local_position.y as f64
			}
		})
	}

	//==============================================================================
	// 끄는 중인지.
	//==============================================================================
	fn is_dragging(&self) -> bool {
		match self.state.upgrade() {
			Some(state) => {
				state.borrow().active_resizer_index == Some(self.resizer_index)
			}
			None => {
				false
			}
		}
	}
}

impl Component for PaneResizerHandle {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"PaneResizerHandle"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		node_tree[node_id].set_interactable(self.can_resize);
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		if !self.can_resize {
			return;
		}
		let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
		let node_id = context.get_node_id();
		self.is_hovered = is_view_position_inside_node(context.get_node_tree(), node_id, view_input_position);
		if self.is_hovered || self.is_dragging() {
			let direction = match self.state.upgrade() {
				Some(state) => {
					state.borrow().direction
				}
				None => {
					return;
				}
			};
			let cursor_name = match direction {
				PaneDirection::Horizontal => {
					"col-resize"
				}
				PaneDirection::Vertical => {
					"row-resize"
				}
			};
			context.get_engine().get_pointer_cursor().request_element_cursor(cursor_name);
		}
	}

	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let state = match self.state.upgrade() {
			Some(state) => {
				state
			}
			None => {
				return;
			}
		};
		let direction = state.borrow().direction;
		if let Some(pointer) = self.read_pointer(context, view_input_position, direction) {
			Pane { state }.begin_resize(self.resizer_index, pointer);
		}
	}

	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let state = match self.state.upgrade() {
			Some(state) => {
				state
			}
			None => {
				return;
			}
		};
		let direction = state.borrow().direction;
		if let Some(pointer) = self.read_pointer(context, view_input_position, direction) {
			Pane { state }.move_resize(context.get_node_tree_mut(), pointer);
		}
	}

	fn touch_release(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		if let Some(state) = self.state.upgrade() {
			Pane { state }.end_resize();
		}
	}

	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		if let Some(state) = self.state.upgrade() {
			Pane { state }.end_resize();
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		let color = if self.can_resize && (self.is_hovered || self.is_dragging()) {
			PaneTheme::COLOR_RESIZER_HOVER
		}
		else {
			PaneTheme::COLOR_RESIZER
		};
		graphic.set_fill_color(color);
		graphic.draw_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y));
	}
}

//==============================================================================
// 요소 표시. (원본 display)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneDisplay {
	Block,
	Flex,
	None,
}

//==============================================================================
// 크기 값. (px, %, auto)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
enum PaneLength {
	Auto,
	Pixel(f32),
	Percent(f32),
}

//==============================================================================
// PaneStyle 이 지은 요소. (원본 DOM 요소의 스타일과 글, 차례대로 놓는 자식 배치, 단추 누르기)
//==============================================================================
pub struct PaneElement {
	component_base: ComponentBase,
	tag_name: String,
	is_absolute: bool,
	left: f32,
	top: f32,
	width: PaneLength,
	height: PaneLength,
	display: PaneDisplay,
	is_justify_center: bool,
	is_align_center: bool,
	gap: f32,
	// 위, 오른쪽, 아래, 왼쪽.
	padding: [f32; 4],
	margin_bottom: f32,
	background_color: Option<Color>,
	border_width: f32,
	border_color: Option<Color>,
	border_bottom_width: f32,
	border_bottom_color: Option<Color>,
	border_radius: f32,
	color: Color,
	font_size: f32,
	is_bold: bool,
	font_family: String,
	text: String,
	is_button: bool,
	is_pointer_cursor: bool,
	is_hovered: bool,
	click_callback: Option<PaneClickCallback>,
}

impl PaneElement {
	//==============================================================================
	// 생성. (원본 PaneStyle.create 의 공통 기본 스타일: absolute, 왼쪽 위, 100%, 13px sans-serif, COLOR_TEXT)
	//==============================================================================
	fn new(tag_name: &str) -> PaneElement {
		PaneElement {
			component_base: ComponentBase::new(),
			tag_name: tag_name.to_ascii_lowercase(),
			is_absolute: true,
			left: 0.0,
			top: 0.0,
			width: PaneLength::Percent(100.0),
			height: PaneLength::Percent(100.0),
			display: PaneDisplay::Block,
			is_justify_center: false,
			is_align_center: false,
			gap: 0.0,
			padding: [0.0; 4],
			margin_bottom: 0.0,
			background_color: None,
			border_width: 0.0,
			border_color: None,
			border_bottom_width: 0.0,
			border_bottom_color: None,
			border_radius: 0.0,
			color: Color::from(PaneTheme::COLOR_TEXT),
			font_size: parse_pixel(PaneTheme::FONT_SIZE).unwrap_or(13.0),
			is_bold: false,
			font_family: String::from(PaneTheme::FONT_FAMILY),
			text: String::new(),
			is_button: false,
			is_pointer_cursor: false,
			is_hovered: false,
			click_callback: None,
		}
	}

	//==============================================================================
	// 누르고 떼면 부를 함수. (원본 요소의 addEventListener("click"))
	//==============================================================================
	pub fn set_on_click(&mut self, click_callback: Option<PaneClickCallback>) {
		self.click_callback = click_callback;
	}

	//==============================================================================
	// 글 설정. (원본 innerText)
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.text = text.to_string();
	}

	//==============================================================================
	// 글.
	//==============================================================================
	pub fn get_text(&self) -> &str {
		&self.text
	}

	//==============================================================================
	// 태그 이름. (소문자)
	//==============================================================================
	pub fn get_tag_name(&self) -> &str {
		&self.tag_name
	}

	//==============================================================================
	// CSS 속성 하나 넣기. (머리 주석의 속성만 읽는다, 그 밖은 무시)
	//==============================================================================
	pub fn apply_style(&mut self, property_name: &str, property_value: &str) {
		let value = property_value.trim();
		match property_name.trim() {
			"position" => {
				self.is_absolute = value != "relative" && value != "static";
			}
			"left" => {
				self.left = parse_pixel(value).unwrap_or(0.0);
			}
			"top" => {
				self.top = parse_pixel(value).unwrap_or(0.0);
			}
			"width" => {
				self.width = parse_length(value);
			}
			"height" => {
				self.height = parse_length(value);
			}
			"display" => {
				self.display = match value {
					"flex" | "inline-flex" => {
						PaneDisplay::Flex
					}
					"none" => {
						PaneDisplay::None
					}
					_ => {
						PaneDisplay::Block
					}
				};
			}
			"justify-content" => {
				self.is_justify_center = value == "center";
			}
			"align-items" => {
				self.is_align_center = value == "center";
			}
			"gap" => {
				self.gap = parse_pixel(value).unwrap_or(0.0);
			}
			"padding" => {
				self.padding = parse_box_values(value);
			}
			"margin-bottom" => {
				self.margin_bottom = parse_pixel(value).unwrap_or(0.0);
			}
			"background" | "background-color" => {
				self.background_color = if value == "transparent" || value == "none" {
					None
				}
				else {
					Some(Color::from(value))
				};
			}
			"color" => {
				self.color = Color::from(value);
			}
			"font-size" => {
				if let Some(font_size) = parse_pixel(value) {
					self.font_size = font_size;
				}
			}
			"font-weight" => {
				self.is_bold = value == "bold" || value.parse::<u32>().map(|weight| weight >= 600).unwrap_or(false);
			}
			"font-family" => {
				self.font_family = value.to_string();
			}
			"border" => {
				let (width, color) = parse_border(value);
				self.border_width = width;
				self.border_color = color;
			}
			"border-bottom" => {
				let (width, color) = parse_border(value);
				self.border_bottom_width = width;
				self.border_bottom_color = color;
			}
			"border-radius" => {
				self.border_radius = parse_pixel(value).unwrap_or(0.0);
			}
			"cursor" => {
				self.is_pointer_cursor = value == "pointer";
			}
			_ => {
			}
		}
	}

	//==============================================================================
	// 폰트 문자열.
	//==============================================================================
	fn build_font_string(&self) -> String {
		let weight = if self.is_bold {
			"bold "
		}
		else {
			""
		};
		format!("{}{}px {}", weight, self.font_size, self.font_family)
	}

	//==============================================================================
	// 줄 높이. (CSS normal 을 글꼴 크기의 1.2 배로 갈음한다)
	//==============================================================================
	fn read_line_height(&self) -> f32 {
		(self.font_size * 1.2).round()
	}

	//==============================================================================
	// 제 크기. (글 폭과 줄 높이에 안쪽 여백과 테두리를 더한다. 입력 칸은 원본 input 의 size 20 처럼 글자 스무 개 폭, 프리셋은 폭 100% 라 부모 폭을 쓴다)
	//==============================================================================
	fn measure_intrinsic_size(&self) -> Vector2 {
		let border_size = self.border_width * 2.0;
		let content_width = if self.tag_name == "input" {
			self.font_size * 0.55 * 20.0
		}
		else {
			match measure_text_without_graphic(&self.build_font_string(), &self.text) {
				Some(text_metrics) => {
					text_metrics.width
				}
				None => {
					self.text.chars().count() as f32 * self.font_size * 0.55
				}
			}
		};
		let content_height = if self.text.is_empty() && self.tag_name != "input" {
			0.0
		}
		else {
			self.read_line_height()
		};
		Vector2::create((content_width + self.padding[1] + self.padding[3] + border_size).ceil(), (content_height + self.padding[0] + self.padding[2] + border_size + self.border_bottom_width).ceil())
	}

	//==============================================================================
	// 폭을 채우는 요소인지. (div 같은 블록 요소, 단추와 입력과 span 은 제 폭)
	//==============================================================================
	fn is_block_level(&self) -> bool {
		!matches!(self.tag_name.as_str(), "button" | "span" | "input" | "label" | "a" | "img")
	}
}

//==============================================================================
// "12px" 같은 값 읽기. (단위 없는 수도 받는다)
//==============================================================================
fn parse_pixel(value: &str) -> Option<f32> {
	value.trim().trim_end_matches("px").trim().parse::<f32>().ok()
}

//==============================================================================
// 크기 값 읽기. (px, %, auto)
//==============================================================================
fn parse_length(value: &str) -> PaneLength {
	let value = value.trim();
	if let Some(percent_text) = value.strip_suffix('%') {
		return percent_text.trim().parse::<f32>().map(PaneLength::Percent).unwrap_or(PaneLength::Auto);
	}
	match parse_pixel(value) {
		Some(pixel) => {
			PaneLength::Pixel(pixel)
		}
		None => {
			PaneLength::Auto
		}
	}
}

//==============================================================================
// 여백 줄임 꼴 읽기. (1 ~ 4 값 → 위, 오른쪽, 아래, 왼쪽)
//==============================================================================
fn parse_box_values(value: &str) -> [f32; 4] {
	let value_list: Vec<f32> = value.split_whitespace().map(|part| parse_pixel(part).unwrap_or(0.0)).collect();
	match value_list.len() {
		1 => {
			[value_list[0]; 4]
		}
		2 => {
			[value_list[0], value_list[1], value_list[0], value_list[1]]
		}
		3 => {
			[value_list[0], value_list[1], value_list[2], value_list[1]]
		}
		4 => {
			[value_list[0], value_list[1], value_list[2], value_list[3]]
		}
		_ => {
			[0.0; 4]
		}
	}
}

//==============================================================================
// 테두리 줄임 꼴 읽기. ("1px solid #444" → 굵기와 색, none 이면 0)
//==============================================================================
fn parse_border(value: &str) -> (f32, Option<Color>) {
	if value.trim() == "none" {
		return (0.0, None);
	}
	let mut width = 0.0;
	let mut color = None;
	for part in value.split_whitespace() {
		if let Some(pixel) = parse_pixel(part) {
			width = pixel;
		}
		else if !matches!(part, "solid" | "dashed" | "dotted" | "double") {
			color = Some(Color::from(part));
		}
	}
	(width, color)
}

//==============================================================================
// 밝기 바꾼 색. (원본 filter: brightness)
//==============================================================================
fn scale_brightness(color: Color, brightness: f32) -> Color {
	Color::create((color.red * brightness).min(1.0), (color.green * brightness).min(1.0), (color.blue * brightness).min(1.0), color.alpha)
}

impl Component for PaneElement {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"PaneElement"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		if self.is_button {
			node_tree[node_id].set_interactable(true);
		}
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let node_id = context.get_node_id();
		// 올려 둠. (단추의 밝기와 pointer 커서)
		if self.is_button || self.is_pointer_cursor {
			let view_input_position = context.get_engine().get_input_manager().get_view_input_position();
			self.is_hovered = is_view_position_inside_node(context.get_node_tree(), node_id, view_input_position);
			if self.is_hovered {
				context.get_engine().get_pointer_cursor().request_element_cursor("pointer");
			}
		}

		// 부모가 PaneElement 가 아니면(패널 컨테이너 같은 것) absolute 요소는 스스로 부모 크기에 맞춘다. (원본 position: absolute 의 100%)
		if self.is_absolute {
			let parent_size = context.get_node_tree().get_parent_id(node_id).and_then(|parent_id| {
				let parent_node = &context.get_node_tree()[parent_id];
				if parent_node.get_component::<PaneElement>().is_some() {
					None
				}
				else {
					Some(parent_node.get_content_size())
				}
			});
			if let Some(parent_size) = parent_size {
				let intrinsic_size = self.measure_intrinsic_size();
				let resolve_length = |length: PaneLength, parent_length: f32, auto_length: f32| -> f32 {
					match length {
						PaneLength::Auto => {
							auto_length
						}
						PaneLength::Pixel(pixel) => {
							pixel
						}
						PaneLength::Percent(percent) => {
							parent_length * percent / 100.0
						}
					}
				};
				let size = Vector2::create(resolve_length(self.width, parent_size.x, intrinsic_size.x), resolve_length(self.height, parent_size.y, intrinsic_size.y));
				let node = context.get_node_mut();
				node.set_local_position(Vector2::create(self.left, self.top));
				node.set_content_size(size);
			}
		}

		// 자식 배치. (absolute 는 부모 크기에, relative 는 안쪽 여백 안에서 차례로)
		let node_size = context.get_node_tree()[node_id].get_content_size();
		let content_left = self.padding[3] + self.border_width;
		let content_top = self.padding[0] + self.border_width;
		let content_width = (node_size.x - self.padding[1] - self.padding[3] - self.border_width * 2.0).max(0.0);
		let content_height = (node_size.y - self.padding[0] - self.padding[2] - self.border_width * 2.0 - self.border_bottom_width).max(0.0);
		let child_id_list: Vec<NodeId> = context.get_node_tree().get_child_ids(node_id).to_vec();
		let mut placement_list: Vec<(NodeId, Option<(Vector2, Vector2)>, f32)> = Vec::new();
		let mut flow_y = content_top;
		let mut flex_item_list: Vec<(NodeId, Vector2)> = Vec::new();
		for child_id in child_id_list {
			let child_element = match context.get_node_tree()[child_id].get_component::<PaneElement>() {
				Some(child_element) => {
					child_element
				}
				None => {
					continue;
				}
			};
			if child_element.display == PaneDisplay::None {
				placement_list.push((child_id, None, 0.0));
				continue;
			}
			let intrinsic_size = child_element.measure_intrinsic_size();
			let resolve_length = |length: PaneLength, parent_length: f32, auto_length: f32| -> f32 {
				match length {
					PaneLength::Auto => {
						auto_length
					}
					PaneLength::Pixel(pixel) => {
						pixel
					}
					PaneLength::Percent(percent) => {
						parent_length * percent / 100.0
					}
				}
			};
			if child_element.is_absolute {
				let size = Vector2::create(resolve_length(child_element.width, node_size.x, intrinsic_size.x), resolve_length(child_element.height, node_size.y, intrinsic_size.y));
				placement_list.push((child_id, Some((Vector2::create(child_element.left, child_element.top), size)), 0.0));
				continue;
			}
			let auto_width = if child_element.is_block_level() && self.display != PaneDisplay::Flex {
				content_width
			}
			else {
				intrinsic_size.x
			};
			let size = Vector2::create(resolve_length(child_element.width, content_width, auto_width), resolve_length(child_element.height, content_height, intrinsic_size.y));
			if self.display == PaneDisplay::Flex {
				flex_item_list.push((child_id, size));
			}
			else {
				placement_list.push((child_id, Some((Vector2::create(content_left, flow_y), size)), 0.0));
				flow_y += size.y + child_element.margin_bottom;
			}
		}
		if !flex_item_list.is_empty() {
			let total_width: f32 = flex_item_list.iter().map(|(_, size)| size.x).sum::<f32>() + self.gap * (flex_item_list.len() as f32 - 1.0);
			let mut flow_x = if self.is_justify_center {
				content_left + (content_width - total_width) * 0.5
			}
			else {
				content_left
			};
			for (child_id, size) in flex_item_list {
				let item_y = if self.is_align_center {
					content_top + (content_height - size.y) * 0.5
				}
				else {
					content_top
				};
				placement_list.push((child_id, Some((Vector2::create(flow_x, item_y), size)), 0.0));
				flow_x += size.x + self.gap;
			}
		}
		let node_tree = context.get_node_tree_mut();
		for (child_id, placement, _) in placement_list {
			let child_node = &mut node_tree[child_id];
			match placement {
				Some((position, size)) => {
					child_node.set_active(true);
					child_node.set_pivot(Pivot::TOP_LEFT);
					child_node.set_anchor(Vector2::zero());
					child_node.set_local_position(position);
					child_node.set_content_size(size);
				}
				None => {
					child_node.set_active(false);
				}
			}
		}
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if !self.is_button {
			return;
		}
		let node_id = context.get_node_id();
		if !is_view_position_inside_node(context.get_node_tree(), node_id, view_input_position) {
			return;
		}
		if let Some(mut click_callback) = self.click_callback.take() {
			click_callback(context);
			if self.click_callback.is_none() {
				self.click_callback = Some(click_callback);
			}
		}
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.display == PaneDisplay::None {
			return;
		}
		let content_size = transform.get_content_size();
		let rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		let brightness = if self.is_button && self.is_hovered {
			1.2
		}
		else {
			1.0
		};
		if let Some(background_color) = self.background_color {
			graphic.set_fill_color(scale_brightness(background_color, brightness));
			if self.border_radius > 0.0 {
				graphic.draw_round_rect(rect, self.border_radius);
			}
			else {
				graphic.draw_rect(rect);
			}
		}
		if let (Some(border_color), true) = (self.border_color, self.border_width > 0.0) {
			graphic.set_stroke_color(border_color);
			let half_width = self.border_width * 0.5;
			graphic.draw_stroke_round_rect(Rect::create(half_width, half_width, content_size.x - self.border_width, content_size.y - self.border_width), (self.border_radius - half_width).max(0.0), self.border_width);
		}
		if let (Some(border_bottom_color), true) = (self.border_bottom_color, self.border_bottom_width > 0.0) {
			graphic.set_fill_color(border_bottom_color);
			graphic.draw_rect(Rect::create(0.0, content_size.y - self.border_bottom_width, content_size.x, self.border_bottom_width));
		}
		if !self.text.is_empty() {
			graphic.set_font_string(&self.build_font_string());
			graphic.set_fill_color(scale_brightness(self.color, brightness));
			graphic.set_text_baseline(TextBaseline::Middle);
			if self.is_button {
				graphic.set_text_align(TextAlign::Center);
				graphic.draw_fill_text(&self.text, content_size.x * 0.5, content_size.y * 0.5);
			}
			else {
				graphic.set_text_align(TextAlign::Left);
				graphic.draw_fill_text(&self.text, self.padding[3] + self.border_width, self.padding[0] + self.border_width + self.read_line_height() * 0.5);
			}
		}
	}
}

//==============================================================================
// 스타일 및 요소 생성 유틸리티. (원본 PaneStyle)
//==============================================================================
pub struct PaneStyle;

impl PaneStyle {
	//==============================================================================
	// 요소에 여러 스타일을 한꺼번에 넣는다. ((CSS 속성 이름, 값) 목록, PaneElement 가 없는 노드면 아무것도 하지 않는다)
	//==============================================================================
	pub fn apply(node_tree: &mut NodeTree, node_id: NodeId, styles: &[(&str, &str)]) {
		if let Some(element) = node_tree.get_node_mut(node_id).and_then(|node| node.get_component_mut::<PaneElement>()) {
			for (property_name, property_value) in styles {
				element.apply_style(property_name, property_value);
			}
		}
	}

	//==============================================================================
	// 스타일 프리셋을 넣은 요소 노드를 만든다. (떨어진 노드, node_tree.add_child 로 붙인다)
	// (preset: "panel" | "toolbar" | "button" | "input" | "textarea" | "text", 그 밖은 공통 스타일만)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, tag_name: &str, preset: &str, options: &PaneStyleOptions) -> NodeId {
		let node_name = if options.id.is_empty() {
			tag_name
		}
		else {
			options.id
		};
		let node_id = node_tree.create_node(node_name);
		{
			let node = &mut node_tree[node_id];
			node.set_pivot(Pivot::TOP_LEFT);
			node.set_anchor(Vector2::zero());
		}
		let mut element = PaneElement::new(tag_name);
		if !options.text.is_empty() {
			element.text = options.text.to_string();
		}
		match preset {
			"toolbar" => {
				element.apply_style("background-color", PaneTheme::COLOR_TOOLBAR);
				element.apply_style("border-bottom", &format!("1px solid {}", PaneTheme::COLOR_BORDER));
				element.apply_style("display", "flex");
				element.apply_style("justify-content", "center");
				element.apply_style("align-items", "center");
				element.apply_style("gap", "10px");
			}
			"panel" => {
				element.apply_style("background-color", PaneTheme::COLOR_PANEL);
				element.apply_style("padding", "10px");
			}
			"button" => {
				let mut background_color = PaneTheme::COLOR_RESIZER;
				if options.accent {
					background_color = PaneTheme::COLOR_ACCENT;
				}
				if options.warning {
					background_color = PaneTheme::COLOR_WARNING;
				}
				element.apply_style("position", "relative");
				element.apply_style("width", "auto");
				element.apply_style("height", "auto");
				element.apply_style("padding", "6px 16px");
				element.apply_style("border", "none");
				element.apply_style("border-radius", "4px");
				element.apply_style("cursor", "pointer");
				element.apply_style("font-weight", "bold");
				element.apply_style("background-color", background_color);
				element.is_button = true;
			}
			"input" => {
				element.apply_style("position", "relative");
				element.apply_style("height", "auto");
				element.apply_style("background", PaneTheme::COLOR_INPUT_BACKGROUND);
				element.apply_style("border", &format!("1px solid {}", PaneTheme::COLOR_RESIZER));
				element.apply_style("padding", "4px");
			}
			"textarea" => {
				element.apply_style("background-color", PaneTheme::COLOR_BACKGROUND);
				element.apply_style("color", "#d4d4d4");
				element.apply_style("font-family", PaneTheme::FONT_MONOSPACE);
				element.apply_style("font-size", "14px");
				element.apply_style("padding", "15px");
				element.apply_style("border", "none");
			}
			"text" => {
				element.apply_style("position", "relative");
				element.apply_style("width", "auto");
				element.apply_style("height", "auto");
				element.apply_style("font-weight", "bold");
			}
			_ => {
			}
		}
		for (property_name, property_value) in options.style {
			element.apply_style(property_name, property_value);
		}
		let is_input = element.tag_name == "input";
		let font_size = element.font_size;
		let text_color = element.color;
		let padding = element.padding[3];
		node_tree.add_component(node_id, element);
		// input 은 엔진 입력 필드가 글을 받는다. (원본 DOM input, 바탕과 테두리는 PaneElement 가 그린다)
		if is_input {
			if let Some(field) = node_tree.add_component(node_id, UIInputField::new()) {
				field.set_background_color(Color::create(0.0, 0.0, 0.0, 0.0));
				field.set_text_color(text_color);
				field.set_font_size(font_size);
				field.set_padding(padding);
				field.set_round_size(0.0);
				field.set_placeholder(options.placeholder);
				field.set_placeholder_color(PaneTheme::COLOR_TEXT_DIM);
				field.set_focus_border_width(1.0);
				field.set_focus_border_color(PaneTheme::COLOR_RESIZER_HOVER);
				field.set_use_dom_input(true);
				field.attach();
			}
		}
		node_id
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	// 노드로 지은 패널: 붙이면 부모 크기로 놓이고, 숨기면 리사이저와 함께 빠지고, 끌면 앞 자식 크기가 바뀐다. 부모 크기가 바뀌면 다시 놓는다.
	//==============================================================================
	#[test]
	fn node_pane_layout_visibility_and_drag() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(1000.0, 600.0));
		let main_pane = Pane::new(&mut node_tree, PaneOptions::default());
		let left_pane = Pane::new(&mut node_tree, PaneOptions { size: PaneSize::Fixed(240.0), min_size: 170.0, ..Default::default() });
		let view_pane = Pane::new(&mut node_tree, PaneOptions { min_size: 200.0, ..Default::default() });
		main_pane.add_pane(&mut node_tree, &left_pane);
		main_pane.add_pane(&mut node_tree, &view_pane);
		let callback_count = Rc::new(std::cell::Cell::new(0));
		let callback_count_for_callback = callback_count.clone();
		main_pane.set_callback(Some(Box::new(move || {
			callback_count_for_callback.set(callback_count_for_callback.get() + 1);
		})));
		main_pane.attach_to(&mut node_tree, root_id);
		assert_eq!(callback_count.get(), 1);
		assert_eq!(node_tree[left_pane.get_container()].get_content_size(), Vector2::create(240.0, 600.0));
		assert_eq!(node_tree[view_pane.get_container()].get_local_position(), Vector2::create(245.0, 0.0));
		assert_eq!(node_tree[view_pane.get_container()].get_content_size(), Vector2::create(755.0, 600.0));
		let resizer_id = node_tree.find_child_by_name(main_pane.get_container(), "paneResizer").expect("resizer");
		assert_eq!(node_tree[resizer_id].get_local_position(), Vector2::create(240.0, 0.0));
		assert!(node_tree[resizer_id].is_interactable());

		// 리사이저를 눌러 끈다. (잡은 자리 2 를 지킨다)
		node_tree.dispatch_touch(resizer_id, &engine, TouchKind::Press, Vector2::create(242.0, 300.0));
		node_tree.dispatch_touch(resizer_id, &engine, TouchKind::Moved, Vector2::create(402.0, 300.0));
		node_tree.dispatch_touch(resizer_id, &engine, TouchKind::Release, Vector2::create(402.0, 300.0));
		assert_eq!(left_pane.get_fixed_size(), 400.0);
		assert_eq!(node_tree[view_pane.get_container()].get_local_position(), Vector2::create(405.0, 0.0));

		// 숨기면 리사이저와 함께 빠지고 남은 패널이 다 채운다. 다시 보이고 초기화하면 처음 크기.
		left_pane.set_visible(&mut node_tree, false);
		assert!(!node_tree[left_pane.get_container()].is_active());
		assert!(!node_tree[resizer_id].is_active());
		assert_eq!(node_tree[view_pane.get_container()].get_local_position(), Vector2::create(0.0, 0.0));
		left_pane.set_visible(&mut node_tree, true);
		main_pane.reset_layout();
		main_pane.refresh(&mut node_tree);
		assert_eq!(node_tree[left_pane.get_container()].get_content_size(), Vector2::create(240.0, 600.0));

		// 부모 크기가 바뀌면 다음 tick 에 다시 놓는다.
		node_tree[root_id].set_content_size(Vector2::create(800.0, 400.0));
		node_tree.tick(&engine, 1.0 / 60.0);
		assert_eq!(node_tree[view_pane.get_container()].get_content_size(), Vector2::create(555.0, 400.0));
	}

	//==============================================================================
	// PaneStyle 프리셋: 도구 막대는 단추를 가로 가운데에 모으고, 패널은 안쪽 여백 안에 차례로, 단추를 누르고 떼면 함수를 부른다.
	//==============================================================================
	#[test]
	fn pane_style_presets_lay_out_like_css() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(600.0, 400.0));
		// 패널 컨테이너 자리. (위 600 x 40, 아래 300 x 360)
		let toolbar_holder_id = node_tree.create_child(root_id, "toolbarHolder");
		let panel_holder_id = node_tree.create_child(root_id, "panelHolder");
		for (holder_id, holder_position, holder_size) in [(toolbar_holder_id, Vector2::create(0.0, 0.0), Vector2::create(600.0, 40.0)), (panel_holder_id, Vector2::create(0.0, 40.0), Vector2::create(300.0, 360.0))] {
			let holder_node = &mut node_tree[holder_id];
			holder_node.set_pivot(Pivot::TOP_LEFT);
			holder_node.set_anchor(Vector2::zero());
			holder_node.set_local_position(holder_position);
			holder_node.set_content_size(holder_size);
		}
		let toolbar_id = PaneStyle::create(&mut node_tree, "div", "toolbar", &PaneStyleOptions::default());
		node_tree.add_child(toolbar_holder_id, toolbar_id);
		let first_button_id = PaneStyle::create(&mut node_tree, "button", "button", &PaneStyleOptions { text: "Toggle", accent: true, ..Default::default() });
		let second_button_id = PaneStyle::create(&mut node_tree, "button", "button", &PaneStyleOptions { text: "Reset", warning: true, ..Default::default() });
		node_tree.add_child(toolbar_id, first_button_id);
		node_tree.add_child(toolbar_id, second_button_id);
		let click_count = Rc::new(std::cell::Cell::new(0));
		let click_count_for_callback = click_count.clone();
		node_tree[first_button_id].get_component_mut::<PaneElement>().expect("element").set_on_click(Some(Box::new(move |_context| {
			click_count_for_callback.set(click_count_for_callback.get() + 1);
		})));
		let panel_id = PaneStyle::create(&mut node_tree, "div", "panel", &PaneStyleOptions::default());
		node_tree.add_child(panel_holder_id, panel_id);
		let title_id = PaneStyle::create(&mut node_tree, "div", "text", &PaneStyleOptions { text: "PRESETS", style: &[("margin-bottom", "8px"), ("font-size", "12px")], ..Default::default() });
		let input_id = PaneStyle::create(&mut node_tree, "input", "input", &PaneStyleOptions { placeholder: "Search", ..Default::default() });
		node_tree.add_child(panel_id, title_id);
		node_tree.add_child(panel_id, input_id);
		// 첫 tick 에 absolute 요소가 부모 크기에 맞고, 다음 tick 에 그 크기로 자식을 놓는다.
		node_tree.tick(&engine, 1.0 / 60.0);
		node_tree.tick(&engine, 1.0 / 60.0);
		assert_eq!(node_tree[toolbar_id].get_content_size(), Vector2::create(600.0, 40.0));

		// 도구 막대: 두 단추가 가로로 10 떨어져 가운데에, 세로도 가운데.
		let first_position = node_tree[first_button_id].get_local_position();
		let first_size = node_tree[first_button_id].get_content_size();
		let second_position = node_tree[second_button_id].get_local_position();
		let second_size = node_tree[second_button_id].get_content_size();
		assert!((second_position.x - (first_position.x + first_size.x + 10.0)).abs() < 0.01);
		assert!(((first_position.x + (second_position.x + second_size.x)) * 0.5 - 300.0).abs() < 0.01);
		assert!(((first_position.y + first_size.y * 0.5) - (40.0 - 1.0) * 0.5).abs() < 0.01);
		assert!(node_tree[first_button_id].is_interactable());

		// 패널: 제목은 안쪽 여백 10 에, 폭을 채우고, 입력 칸은 그 아래 margin-bottom 8 뒤.
		assert_eq!(node_tree[title_id].get_local_position(), Vector2::create(10.0, 10.0));
		assert_eq!(node_tree[title_id].get_content_size().x, 280.0);
		let title_height = node_tree[title_id].get_content_size().y;
		assert_eq!(node_tree[input_id].get_local_position(), Vector2::create(10.0, 10.0 + title_height + 8.0));
		assert!(node_tree[input_id].get_component::<UIInputField>().is_some());

		// 단추 누르고 떼기.
		let button_center = Vector2::create(first_position.x + first_size.x * 0.5, first_position.y + first_size.y * 0.5);
		node_tree.dispatch_touch(first_button_id, &engine, TouchKind::Press, button_center);
		node_tree.dispatch_touch(first_button_id, &engine, TouchKind::Release, button_center);
		assert_eq!(click_count.get(), 1);
		PaneStyle::apply(&mut node_tree, first_button_id, &[("display", "none")]);
		node_tree.tick(&engine, 1.0 / 60.0);
		assert!(!node_tree[first_button_id].is_active());
	}
}
