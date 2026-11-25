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
//   relative 요소는 부모 안쪽 여백 안에서 놓는다. 블록 부모는 세로로 차례로(div 는 폭을 채우고 그 밖은 제 폭), flex 부모는 CSS Flexbox 처럼
//   주축(flex-direction row, column)으로 기준 크기(flex-basis, 없으면 width 나 height, 그것도 auto 면 제 크기)를 놓고 남는 자리를 flex-grow 비로 나누거나
//   모자라면 flex-shrink 와 기준 크기를 곱한 비로 줄인다. (min, max 에 걸린 항목은 묶고 다시 나눈다, 넘침이 visible 인 항목의 자동 최소는 제 크기)
//   교차축은 align-items(기본 stretch, flex-start, center, flex-end), 주축 늘어놓기는 justify-content(flex-start, center, flex-end, space-between, space-around, space-evenly)와 gap.
//   auto 크기 요소의 제 크기는 글과 자식(블록은 세로 합, 가로 flex 는 가로 합)에 안쪽 여백과 테두리를 더한 것이다.
// - overflow(overflow-x, overflow-y)가 visible 이 아니면 넘친 자식을 자르고(모서리 border-radius), auto, scroll 이면 휠로 넘기고 썸을 끌어 넘긴다.
//   원본 스크롤 상자처럼 그 쪽으로 더 넘길 수 없으면 휠을 부모에게 넘긴다. 썸은 scrollbar-color 의 썸 색(원본 에디터의 rgba(121, 121, 121, 0.4))이다.
// - 원본과 다른 점:
//   - 원본 요소에 addEventListener("click") 으로 다는 것은 PaneElement::set_on_click 이다.
//   - options.style 과 PaneStyle::apply 가 읽는 CSS 는 프리셋과 원본 도구(파티클, 타임라인, UI 에디터)가 쓰는 것이다:
//     position(absolute, relative), left, top, right, bottom, width, height, min-width, min-height, max-width, max-height(px, %, auto),
//     display(flex, block, none), flex-direction, flex, flex-grow, flex-shrink, flex-basis, justify-content, align-items, gap,
//     padding, margin(1 ~ 4 값과 쪽마다), background, background-color, color, font-size, font-weight, font-family, line-height, text-align,
//     border, border-top, border-right, border-bottom, border-left, border-width, border-color, border-radius, cursor(pointer),
//     overflow, overflow-x, overflow-y, scrollbar-color. 그 밖(flex-wrap 줄넘김, white-space 와 text-overflow 의 말줄임, user-select, opacity 같은 것)은 읽지 않는다.
//   - 스크롤 막대는 자리를 차지하지 않고 내용 위에 얹는 두께 8 의 썸이다. (크롬은 운영체제와 scrollbar-width 에 따라 자리를 차지하는 막대일 수 있다)
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

impl PaneLength {
	//==============================================================================
	// 길이 셈. (auto 면 None, %는 부모 길이에 대고)
	//==============================================================================
	fn resolve(self, parent_length: f32) -> Option<f32> {
		match self {
			PaneLength::Auto => {
				None
			}
			PaneLength::Pixel(pixel) => {
				Some(pixel)
			}
			PaneLength::Percent(percent) => {
				Some(parent_length * percent / 100.0)
			}
		}
	}
}

//==============================================================================
// flex 주축. (원본 flex-direction)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneFlexDirection {
	Row,
	Column,
}

//==============================================================================
// 주축 늘어놓기. (원본 justify-content)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneJustifyContent {
	Start,
	Center,
	End,
	SpaceBetween,
	SpaceAround,
	SpaceEvenly,
}

//==============================================================================
// 교차축 맞추기. (원본 align-items, 기본 stretch)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneAlignItems {
	Stretch,
	Start,
	Center,
	End,
}

//==============================================================================
// 넘침. (원본 overflow, overflow-x, overflow-y)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneOverflow {
	Visible,
	Hidden,
	Auto,
	Scroll,
}

impl PaneOverflow {
	//==============================================================================
	// 읽기. (clip 은 hidden 과 같이 자른다, 모르는 값은 visible)
	//==============================================================================
	fn parse(value: &str) -> PaneOverflow {
		match value {
			"hidden" | "clip" => {
				PaneOverflow::Hidden
			}
			"auto" | "overlay" => {
				PaneOverflow::Auto
			}
			"scroll" => {
				PaneOverflow::Scroll
			}
			_ => {
				PaneOverflow::Visible
			}
		}
	}

	//==============================================================================
	// 굴려 넘길 수 있는지. (auto, scroll)
	//==============================================================================
	fn is_scrollable(self) -> bool {
		matches!(self, PaneOverflow::Auto | PaneOverflow::Scroll)
	}
}

//==============================================================================
// 글 가로 맞춤. (원본 text-align)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneTextAlign {
	Left,
	Center,
	Right,
}

// 스크롤 막대 두께와 가장 짧은 썸 길이. (얹는 막대)
const SCROLLBAR_THICKNESS: f32 = 8.0;
const SCROLLBAR_MINIMUM_THUMB_LENGTH: f32 = 20.0;
// 스크롤 막대 썸 기본 색. (원본 에디터의 scrollbar-color 썸)
const SCROLLBAR_DEFAULT_THUMB_COLOR: &str = "rgba(121, 121, 121, 0.4)";

//==============================================================================
// PaneStyle 이 지은 요소. (원본 DOM 요소의 스타일과 글, 블록과 flex 배치, 넘침 자르기와 스크롤, 단추 누르기)
//==============================================================================
pub struct PaneElement {
	component_base: ComponentBase,
	tag_name: String,
	is_absolute: bool,
	// 자리. (absolute 요소, None 이면 auto)
	left: Option<f32>,
	top: Option<f32>,
	right: Option<f32>,
	bottom: Option<f32>,
	width: PaneLength,
	height: PaneLength,
	minimum_width: PaneLength,
	minimum_height: PaneLength,
	maximum_width: PaneLength,
	maximum_height: PaneLength,
	display: PaneDisplay,
	flex_direction: PaneFlexDirection,
	justify_content: PaneJustifyContent,
	align_items: PaneAlignItems,
	gap: f32,
	flex_grow: f32,
	flex_shrink: f32,
	flex_basis: PaneLength,
	// 위, 오른쪽, 아래, 왼쪽.
	padding: [f32; 4],
	margin: [f32; 4],
	border_width_list: [f32; 4],
	border_color_list: [Option<Color>; 4],
	background_color: Option<Color>,
	border_radius: f32,
	color: Color,
	font_size: f32,
	is_bold: bool,
	font_family: String,
	line_height: Option<f32>,
	text_align: PaneTextAlign,
	overflow_x: PaneOverflow,
	overflow_y: PaneOverflow,
	scrollbar_thumb_color: Color,
	// 굴려 넘긴 만큼과 내용 끝. (안쪽 여백 상자 기준)
	scroll_offset: Vector2,
	scroll_extent: Vector2,
	scroll_drag_start: Option<(Vector2, Vector2, bool)>,
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
			left: Some(0.0),
			top: Some(0.0),
			right: None,
			bottom: None,
			width: PaneLength::Percent(100.0),
			height: PaneLength::Percent(100.0),
			minimum_width: PaneLength::Auto,
			minimum_height: PaneLength::Auto,
			maximum_width: PaneLength::Auto,
			maximum_height: PaneLength::Auto,
			display: PaneDisplay::Block,
			flex_direction: PaneFlexDirection::Row,
			justify_content: PaneJustifyContent::Start,
			align_items: PaneAlignItems::Stretch,
			gap: 0.0,
			flex_grow: 0.0,
			flex_shrink: 1.0,
			flex_basis: PaneLength::Auto,
			padding: [0.0; 4],
			margin: [0.0; 4],
			border_width_list: [0.0; 4],
			border_color_list: [None; 4],
			background_color: None,
			border_radius: 0.0,
			color: Color::from(PaneTheme::COLOR_TEXT),
			font_size: parse_pixel(PaneTheme::FONT_SIZE).unwrap_or(13.0),
			is_bold: false,
			font_family: String::from(PaneTheme::FONT_FAMILY),
			line_height: None,
			text_align: PaneTextAlign::Left,
			overflow_x: PaneOverflow::Visible,
			overflow_y: PaneOverflow::Visible,
			scrollbar_thumb_color: Color::from(SCROLLBAR_DEFAULT_THUMB_COLOR),
			scroll_offset: Vector2::zero(),
			scroll_extent: Vector2::zero(),
			scroll_drag_start: None,
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
	// 굴려 넘긴 만큼. (원본 scrollLeft, scrollTop)
	//==============================================================================
	pub fn get_scroll_offset(&self) -> Vector2 {
		self.scroll_offset
	}

	//==============================================================================
	// 굴려 넘기기. (원본 scrollLeft, scrollTop 넣기. 다음 배치에서 내용 끝 안으로 줄인다)
	//==============================================================================
	pub fn set_scroll_offset(&mut self, scroll_offset: Vector2) {
		self.scroll_offset = Vector2::create(scroll_offset.x.max(0.0), scroll_offset.y.max(0.0));
	}

	//==============================================================================
	// CSS 속성 하나 넣기. (머리 주석의 속성만 읽는다, 그 밖은 무시)
	//==============================================================================
	pub fn apply_style(&mut self, property_name: &str, property_value: &str) {
		let value = property_value.trim();
		match property_name.trim() {
			"position" => {
				self.is_absolute = value == "absolute" || value == "fixed";
			}
			"left" => {
				self.left = parse_pixel(value);
			}
			"top" => {
				self.top = parse_pixel(value);
			}
			"right" => {
				self.right = parse_pixel(value);
			}
			"bottom" => {
				self.bottom = parse_pixel(value);
			}
			"width" => {
				self.width = parse_length(value);
			}
			"height" => {
				self.height = parse_length(value);
			}
			"min-width" => {
				self.minimum_width = parse_length(value);
			}
			"min-height" => {
				self.minimum_height = parse_length(value);
			}
			"max-width" => {
				self.maximum_width = parse_length(value);
			}
			"max-height" => {
				self.maximum_height = parse_length(value);
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
			"flex-direction" => {
				self.flex_direction = if value.starts_with("column") {
					PaneFlexDirection::Column
				}
				else {
					PaneFlexDirection::Row
				};
			}
			"justify-content" => {
				self.justify_content = match value {
					"center" => {
						PaneJustifyContent::Center
					}
					"flex-end" | "end" | "right" => {
						PaneJustifyContent::End
					}
					"space-between" => {
						PaneJustifyContent::SpaceBetween
					}
					"space-around" => {
						PaneJustifyContent::SpaceAround
					}
					"space-evenly" => {
						PaneJustifyContent::SpaceEvenly
					}
					_ => {
						PaneJustifyContent::Start
					}
				};
			}
			"align-items" => {
				self.align_items = match value {
					"center" => {
						PaneAlignItems::Center
					}
					"flex-start" | "start" | "self-start" | "baseline" => {
						PaneAlignItems::Start
					}
					"flex-end" | "end" | "self-end" => {
						PaneAlignItems::End
					}
					_ => {
						PaneAlignItems::Stretch
					}
				};
			}
			"gap" => {
				self.gap = parse_pixel(value.split_whitespace().next().unwrap_or("")).unwrap_or(0.0);
			}
			"flex" => {
				self.apply_flex_shorthand(value);
			}
			"flex-grow" => {
				self.flex_grow = value.parse::<f32>().unwrap_or(0.0).max(0.0);
			}
			"flex-shrink" => {
				self.flex_shrink = value.parse::<f32>().unwrap_or(1.0).max(0.0);
			}
			"flex-basis" => {
				self.flex_basis = parse_length(value);
			}
			"padding" => {
				self.padding = parse_box_values(value);
			}
			"padding-top" => {
				self.padding[0] = parse_pixel(value).unwrap_or(0.0);
			}
			"padding-right" => {
				self.padding[1] = parse_pixel(value).unwrap_or(0.0);
			}
			"padding-bottom" => {
				self.padding[2] = parse_pixel(value).unwrap_or(0.0);
			}
			"padding-left" => {
				self.padding[3] = parse_pixel(value).unwrap_or(0.0);
			}
			"margin" => {
				self.margin = parse_box_values(value);
			}
			"margin-top" => {
				self.margin[0] = parse_pixel(value).unwrap_or(0.0);
			}
			"margin-right" => {
				self.margin[1] = parse_pixel(value).unwrap_or(0.0);
			}
			"margin-bottom" => {
				self.margin[2] = parse_pixel(value).unwrap_or(0.0);
			}
			"margin-left" => {
				self.margin[3] = parse_pixel(value).unwrap_or(0.0);
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
				self.is_bold = value == "bold" || value == "bolder" || value.parse::<u32>().map(|weight| weight >= 600).unwrap_or(false);
			}
			"font-family" => {
				self.font_family = value.to_string();
			}
			"line-height" => {
				self.line_height = if value == "normal" {
					None
				}
				else if value.ends_with("px") {
					parse_pixel(value)
				}
				else {
					value.parse::<f32>().ok().map(|multiplier| multiplier * self.font_size)
				};
			}
			"text-align" => {
				self.text_align = match value {
					"center" => {
						PaneTextAlign::Center
					}
					"right" | "end" => {
						PaneTextAlign::Right
					}
					_ => {
						PaneTextAlign::Left
					}
				};
			}
			"border" => {
				let (width, color) = parse_border(value);
				self.border_width_list = [width; 4];
				self.border_color_list = [color; 4];
			}
			"border-top" | "border-right" | "border-bottom" | "border-left" => {
				let side_index = match property_name.trim() {
					"border-top" => {
						0
					}
					"border-right" => {
						1
					}
					"border-bottom" => {
						2
					}
					_ => {
						3
					}
				};
				let (width, color) = parse_border(value);
				self.border_width_list[side_index] = width;
				self.border_color_list[side_index] = color;
			}
			"border-width" => {
				self.border_width_list = parse_box_values(value);
			}
			"border-color" => {
				let color = Color::from(value);
				self.border_color_list = [Some(color); 4];
			}
			"border-radius" => {
				self.border_radius = parse_pixel(value.split_whitespace().next().unwrap_or("")).unwrap_or(0.0);
			}
			"cursor" => {
				self.is_pointer_cursor = value == "pointer";
			}
			"overflow" => {
				let mut part_list = value.split_whitespace();
				let first_overflow = PaneOverflow::parse(part_list.next().unwrap_or(""));
				let second_overflow = part_list.next().map(PaneOverflow::parse).unwrap_or(first_overflow);
				self.overflow_x = first_overflow;
				self.overflow_y = second_overflow;
				self.normalize_overflow();
			}
			"overflow-x" => {
				self.overflow_x = PaneOverflow::parse(value);
				self.normalize_overflow();
			}
			"overflow-y" => {
				self.overflow_y = PaneOverflow::parse(value);
				self.normalize_overflow();
			}
			"scrollbar-color" => {
				if let Some(thumb_text) = split_css_values(value).first() {
					if *thumb_text != "auto" {
						self.scrollbar_thumb_color = Color::from(thumb_text.as_str());
					}
				}
			}
			_ => {
			}
		}
	}

	//==============================================================================
	// flex 줄임 꼴. (원본 flex: none, auto, 수 하나(grow, basis 0%), grow shrink, grow shrink basis, 길이 하나(basis))
	//==============================================================================
	fn apply_flex_shorthand(&mut self, value: &str) {
		match value {
			"none" => {
				self.flex_grow = 0.0;
				self.flex_shrink = 0.0;
				self.flex_basis = PaneLength::Auto;
				return;
			}
			"auto" => {
				self.flex_grow = 1.0;
				self.flex_shrink = 1.0;
				self.flex_basis = PaneLength::Auto;
				return;
			}
			"initial" => {
				self.flex_grow = 0.0;
				self.flex_shrink = 1.0;
				self.flex_basis = PaneLength::Auto;
				return;
			}
			_ => {
			}
		}
		let mut number_list: Vec<f32> = Vec::new();
		let mut basis: Option<PaneLength> = None;
		for part in value.split_whitespace() {
			match part.parse::<f32>() {
				Ok(number) if number_list.len() < 2 => {
					number_list.push(number.max(0.0));
				}
				_ => {
					basis = Some(parse_length(part));
				}
			}
		}
		self.flex_grow = number_list.first().copied().unwrap_or(1.0);
		self.flex_shrink = number_list.get(1).copied().unwrap_or(1.0);
		self.flex_basis = match basis {
			Some(basis) => {
				basis
			}
			None if !number_list.is_empty() => {
				PaneLength::Percent(0.0)
			}
			None => {
				PaneLength::Auto
			}
		};
	}

	//==============================================================================
	// 한 축만 visible 이면 auto 로. (CSS: 다른 축이 visible 이 아니면 visible 은 auto 로 계산된다)
	//==============================================================================
	fn normalize_overflow(&mut self) {
		if self.overflow_x == PaneOverflow::Visible && self.overflow_y != PaneOverflow::Visible {
			self.overflow_x = PaneOverflow::Auto;
		}
		if self.overflow_y == PaneOverflow::Visible && self.overflow_x != PaneOverflow::Visible {
			self.overflow_y = PaneOverflow::Auto;
		}
	}

	//==============================================================================
	// 넘친 자식을 자르는지. (overflow 가 visible 이 아님)
	//==============================================================================
	fn is_clipping(&self) -> bool {
		self.overflow_x != PaneOverflow::Visible || self.overflow_y != PaneOverflow::Visible
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
	// 줄 높이. (line-height 가 없으면 CSS normal 을 글꼴 크기의 1.2 배로 갈음한다)
	//==============================================================================
	fn read_line_height(&self) -> f32 {
		match self.line_height {
			Some(line_height) => {
				line_height
			}
			None => {
				(self.font_size * 1.2).round()
			}
		}
	}

	//==============================================================================
	// 글 크기. (글 폭과 줄 높이, 입력 칸은 원본 input 의 size 20 처럼 글자 스무 개 폭)
	//==============================================================================
	fn measure_text_size(&self) -> Vector2 {
		let text_width = if self.tag_name == "input" {
			self.font_size * 0.55 * 20.0
		}
		else if self.text.is_empty() {
			0.0
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
		let text_height = if self.text.is_empty() && self.tag_name != "input" && self.tag_name != "textarea" {
			0.0
		}
		else {
			self.read_line_height()
		};
		Vector2::create(text_width, text_height)
	}

	//==============================================================================
	// 안쪽 여백과 테두리를 더한 둘레. (가로 합, 세로 합)
	//==============================================================================
	fn read_frame_size(&self) -> Vector2 {
		Vector2::create(self.padding[1] + self.padding[3] + self.border_width_list[1] + self.border_width_list[3], self.padding[0] + self.padding[2] + self.border_width_list[0] + self.border_width_list[2])
	}

	//==============================================================================
	// 폭을 채우는 요소인지. (div 같은 블록 요소, 단추와 입력과 span 은 제 폭)
	//==============================================================================
	fn is_block_level(&self) -> bool {
		!matches!(self.tag_name.as_str(), "button" | "span" | "input" | "label" | "a" | "img" | "canvas")
	}

	//==============================================================================
	// 스크롤 막대 썸 자리. (노드 좌표, 세로 막대면 is_vertical, 넘치지 않으면 None)
	//==============================================================================
	fn calculate_scrollbar_thumb_rect(&self, content_size: Vector2, is_vertical: bool) -> Option<Rect> {
		let (overflow, viewport_length, extent_length, offset) = if is_vertical {
			(self.overflow_y, content_size.y - self.border_width_list[0] - self.border_width_list[2], self.scroll_extent.y, self.scroll_offset.y)
		}
		else {
			(self.overflow_x, content_size.x - self.border_width_list[1] - self.border_width_list[3], self.scroll_extent.x, self.scroll_offset.x)
		};
		if !overflow.is_scrollable() || extent_length <= viewport_length + 0.5 || viewport_length <= 0.0 {
			return None;
		}
		let track_length = viewport_length;
		let thumb_length = (track_length * viewport_length / extent_length).max(SCROLLBAR_MINIMUM_THUMB_LENGTH).min(track_length);
		let maximum_offset = extent_length - viewport_length;
		let thumb_start = (track_length - thumb_length) * (offset / maximum_offset).clamp(0.0, 1.0);
		if is_vertical {
			Some(Rect::create(content_size.x - self.border_width_list[1] - SCROLLBAR_THICKNESS, self.border_width_list[0] + thumb_start, SCROLLBAR_THICKNESS, thumb_length))
		}
		else {
			Some(Rect::create(self.border_width_list[3] + thumb_start, content_size.y - self.border_width_list[2] - SCROLLBAR_THICKNESS, thumb_length, SCROLLBAR_THICKNESS))
		}
	}

	//==============================================================================
	// 굴려 넘긴 만큼을 내용 끝 안으로. (넘길 수 없는 축은 0, 바뀌었는지)
	//==============================================================================
	fn clamp_scroll_offset(&mut self, viewport_size: Vector2) -> bool {
		let maximum_x = if self.overflow_x.is_scrollable() {
			(self.scroll_extent.x - viewport_size.x).max(0.0)
		}
		else {
			0.0
		};
		let maximum_y = if self.overflow_y.is_scrollable() {
			(self.scroll_extent.y - viewport_size.y).max(0.0)
		}
		else {
			0.0
		};
		let clamped_offset = Vector2::create(self.scroll_offset.x.clamp(0.0, maximum_x), self.scroll_offset.y.clamp(0.0, maximum_y));
		let is_changed = clamped_offset.x != self.scroll_offset.x || clamped_offset.y != self.scroll_offset.y;
		self.scroll_offset = clamped_offset;
		is_changed
	}
}

//==============================================================================
// 요소 제 크기 재기. (원본 max-content 크기: 글, 자식 블록과 flex 의 합, 안쪽 여백과 테두리. 명시한 px 폭과 높이, min, max 를 따른다)
// - %는 부모를 알 수 없어 auto 로 본다. absolute 와 display none 자식은 넣지 않는다.
//==============================================================================
fn measure_element_size(node_tree: &NodeTree, node_id: NodeId) -> Vector2 {
	let element = match node_tree[node_id].get_component::<PaneElement>() {
		Some(element) => {
			element
		}
		None => {
			return node_tree[node_id].get_content_size();
		}
	};
	let text_size = element.measure_text_size();
	let mut children_width: f32 = 0.0;
	let mut children_height: f32 = 0.0;
	let mut flow_count = 0;
	for child_id in node_tree.get_child_ids(node_id).iter().copied() {
		let child_element = match node_tree[child_id].get_component::<PaneElement>() {
			Some(child_element) => {
				child_element
			}
			None => {
				continue;
			}
		};
		if child_element.display == PaneDisplay::None || child_element.is_absolute {
			continue;
		}
		let child_size = measure_element_size(node_tree, child_id);
		let child_outer_width = child_size.x + child_element.margin[1] + child_element.margin[3];
		let child_outer_height = child_size.y + child_element.margin[0] + child_element.margin[2];
		let gap = if flow_count > 0 && element.display == PaneDisplay::Flex {
			element.gap
		}
		else {
			0.0
		};
		if element.display == PaneDisplay::Flex && element.flex_direction == PaneFlexDirection::Row {
			children_width += child_outer_width + gap;
			children_height = children_height.max(child_outer_height);
		}
		else {
			children_width = children_width.max(child_outer_width);
			children_height += child_outer_height + gap;
		}
		flow_count += 1;
	}
	let frame_size = element.read_frame_size();
	let mut width = text_size.x.max(children_width) + frame_size.x;
	let mut height = text_size.y + children_height + frame_size.y;
	if let PaneLength::Pixel(pixel) = element.width {
		width = pixel;
	}
	if let PaneLength::Pixel(pixel) = element.height {
		height = pixel;
	}
	if let PaneLength::Pixel(maximum_width) = element.maximum_width {
		width = width.min(maximum_width);
	}
	if let PaneLength::Pixel(maximum_height) = element.maximum_height {
		height = height.min(maximum_height);
	}
	if let PaneLength::Pixel(minimum_width) = element.minimum_width {
		width = width.max(minimum_width);
	}
	if let PaneLength::Pixel(minimum_height) = element.minimum_height {
		height = height.max(minimum_height);
	}
	Vector2::create(width.ceil(), height.ceil())
}

//==============================================================================
// flex 항목 하나. (배치 셈 동안)
//==============================================================================
struct PaneFlexItem {
	node_id: NodeId,
	// 주축과 교차축의 바깥 여백. (앞, 뒤)
	main_margin: (f32, f32),
	cross_margin: (f32, f32),
	base_size: f32,
	minimum_main: f32,
	maximum_main: f32,
	flex_grow: f32,
	flex_shrink: f32,
	main_size: f32,
	cross_size: f32,
}

//==============================================================================
// 값을 min, max 안으로. (max 가 min 보다 작으면 min 이 이긴다, CSS 와 같다)
//==============================================================================
fn clamp_length(value: f32, minimum: f32, maximum: f32) -> f32 {
	value.min(maximum).max(minimum)
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
// CSS 값 나누기. (빈칸으로 나누되 괄호 안의 빈칸은 두고, "rgba(1, 2, 3, 0.4) transparent" → 둘)
//==============================================================================
fn split_css_values(value: &str) -> Vec<String> {
	let mut part_list: Vec<String> = Vec::new();
	let mut current_part = String::new();
	let mut depth = 0;
	for character in value.chars() {
		match character {
			'(' => {
				depth += 1;
				current_part.push(character);
			}
			')' => {
				depth -= 1;
				current_part.push(character);
			}
			_ if character.is_whitespace() && depth == 0 => {
				if !current_part.is_empty() {
					part_list.push(std::mem::take(&mut current_part));
				}
			}
			_ => {
				current_part.push(character);
			}
		}
	}
	if !current_part.is_empty() {
		part_list.push(current_part);
	}
	part_list
}

//==============================================================================
// 테두리 줄임 꼴 읽기. ("1px solid #444" → 굵기와 색, none 이면 0)
//==============================================================================
fn parse_border(value: &str) -> (f32, Option<Color>) {
	if value.trim() == "none" || value.trim() == "0" {
		return (0.0, None);
	}
	let mut width = 0.0;
	let mut color = None;
	for part in split_css_values(value) {
		if let Some(pixel) = parse_pixel(&part) {
			width = pixel;
		}
		else if !matches!(part.as_str(), "solid" | "dashed" | "dotted" | "double" | "none") {
			color = Some(Color::from(part.as_str()));
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
		if self.is_button || self.is_clipping() {
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
		// 스타일이 나중에 overflow 를 넣었으면 스크롤 막대를 끌 수 있게 누름을 받는다.
		if self.is_clipping() && !context.get_node_tree()[node_id].is_interactable() {
			context.get_node_mut().set_interactable(true);
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
				let intrinsic_size = measure_element_size(context.get_node_tree(), node_id);
				let (position, size) = self.place_absolute(parent_size, intrinsic_size);
				let node = context.get_node_mut();
				node.set_local_position(position);
				node.set_content_size(size);
			}
		}

		self.lay_out_children(context);
	}

	fn touch_press(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		self.scroll_drag_start = None;
		if !self.is_clipping() {
			return;
		}
		let node_id = context.get_node_id();
		let content_size = context.get_node_tree()[node_id].get_content_size();
		let local_position = match convert_view_to_local(context.get_node_tree(), node_id, view_input_position) {
			Some(local_position) => {
				local_position
			}
			None => {
				return;
			}
		};
		for is_vertical in [true, false] {
			if let Some(thumb_rect) = self.calculate_scrollbar_thumb_rect(content_size, is_vertical) {
				let is_inside = local_position.x >= thumb_rect.position.x && local_position.x <= thumb_rect.position.x + thumb_rect.size.x && local_position.y >= thumb_rect.position.y && local_position.y <= thumb_rect.position.y + thumb_rect.size.y;
				if is_inside {
					self.scroll_drag_start = Some((local_position, self.scroll_offset, is_vertical));
					return;
				}
			}
		}
	}

	fn touch_move(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		let (start_position, start_offset, is_vertical) = match self.scroll_drag_start {
			Some(scroll_drag_start) => {
				scroll_drag_start
			}
			None => {
				return;
			}
		};
		let node_id = context.get_node_id();
		let content_size = context.get_node_tree()[node_id].get_content_size();
		let local_position = match convert_view_to_local(context.get_node_tree(), node_id, view_input_position) {
			Some(local_position) => {
				local_position
			}
			None => {
				return;
			}
		};
		// 썸을 옮긴 만큼을 내용 길이의 비로 넘긴다.
		let viewport_size = Vector2::create(content_size.x - self.border_width_list[1] - self.border_width_list[3], content_size.y - self.border_width_list[0] - self.border_width_list[2]);
		if is_vertical {
			let thumb_length = (viewport_size.y * viewport_size.y / self.scroll_extent.y.max(1.0)).max(SCROLLBAR_MINIMUM_THUMB_LENGTH).min(viewport_size.y);
			let track_room = (viewport_size.y - thumb_length).max(1.0);
			let maximum_offset = (self.scroll_extent.y - viewport_size.y).max(0.0);
			self.scroll_offset.y = start_offset.y + (local_position.y - start_position.y) / track_room * maximum_offset;
		}
		else {
			let thumb_length = (viewport_size.x * viewport_size.x / self.scroll_extent.x.max(1.0)).max(SCROLLBAR_MINIMUM_THUMB_LENGTH).min(viewport_size.x);
			let track_room = (viewport_size.x - thumb_length).max(1.0);
			let maximum_offset = (self.scroll_extent.x - viewport_size.x).max(0.0);
			self.scroll_offset.x = start_offset.x + (local_position.x - start_position.x) / track_room * maximum_offset;
		}
		self.clamp_scroll_offset(viewport_size);
		self.lay_out_children(context);
	}

	fn touch_release(&mut self, context: &mut ComponentContext, view_input_position: Vector2) {
		if self.scroll_drag_start.take().is_some() {
			return;
		}
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

	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.scroll_drag_start = None;
	}

	fn is_wheel_receiver(&self) -> bool {
		self.overflow_x.is_scrollable() || self.overflow_y.is_scrollable()
	}

	//==============================================================================
	// 휠. (원본 스크롤 상자처럼 그 축으로 더 넘길 수 있을 때만 받고, 아니면 부모에게 넘긴다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		if !self.is_wheel_receiver() {
			return false;
		}
		let node_id = context.get_node_id();
		let content_size = context.get_node_tree()[node_id].get_content_size();
		let viewport_size = Vector2::create(content_size.x - self.border_width_list[1] - self.border_width_list[3], content_size.y - self.border_width_list[0] - self.border_width_list[2]);
		let previous_offset = self.scroll_offset;
		self.scroll_offset = Vector2::create(self.scroll_offset.x + wheel_delta.x, self.scroll_offset.y + wheel_delta.y);
		self.clamp_scroll_offset(viewport_size);
		if self.scroll_offset.x == previous_offset.x && self.scroll_offset.y == previous_offset.y {
			return false;
		}
		self.lay_out_children(context);
		true
	}

	fn get_clip_round_size(&self) -> Option<f32> {
		if self.is_clipping() {
			Some(self.border_radius)
		}
		else {
			None
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
		self.draw_border(content_size, graphic);
		if !self.text.is_empty() {
			graphic.set_font_string(&self.build_font_string());
			graphic.set_fill_color(scale_brightness(self.color, brightness));
			graphic.set_text_baseline(TextBaseline::Middle);
			let frame_left = self.padding[3] + self.border_width_list[3];
			let frame_top = self.padding[0] + self.border_width_list[0];
			let inner_width = (content_size.x - self.read_frame_size().x).max(0.0);
			let inner_height = (content_size.y - self.read_frame_size().y).max(0.0);
			// 글 자리. (단추는 가운데, flex 상자의 글은 익명 항목처럼 justify-content 와 align-items 를 따르고, 블록은 text-align 과 맨 위 줄)
			let is_flex = self.display == PaneDisplay::Flex;
			let horizontal_align = if self.is_button || (is_flex && self.justify_content == PaneJustifyContent::Center) || self.text_align == PaneTextAlign::Center {
				PaneTextAlign::Center
			}
			else if (is_flex && self.justify_content == PaneJustifyContent::End) || self.text_align == PaneTextAlign::Right {
				PaneTextAlign::Right
			}
			else {
				PaneTextAlign::Left
			};
			let text_y = if self.is_button || (is_flex && self.align_items == PaneAlignItems::Center) {
				frame_top + inner_height * 0.5
			}
			else {
				frame_top + self.read_line_height() * 0.5
			};
			match horizontal_align {
				PaneTextAlign::Left => {
					graphic.set_text_align(TextAlign::Left);
					graphic.draw_fill_text(&self.text, frame_left, text_y);
				}
				PaneTextAlign::Center => {
					graphic.set_text_align(TextAlign::Center);
					graphic.draw_fill_text(&self.text, frame_left + inner_width * 0.5, text_y);
				}
				PaneTextAlign::Right => {
					graphic.set_text_align(TextAlign::Right);
					graphic.draw_fill_text(&self.text, frame_left + inner_width, text_y);
				}
			}
		}
	}

	//==============================================================================
	// 자식을 그린 뒤 스크롤 막대. (자르기가 끝난 뒤라 막대가 내용 위에 얹힌다)
	//==============================================================================
	fn pop_transform(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.display == PaneDisplay::None || !self.is_clipping() {
			return;
		}
		let content_size = transform.get_content_size();
		for is_vertical in [true, false] {
			if let Some(thumb_rect) = self.calculate_scrollbar_thumb_rect(content_size, is_vertical) {
				graphic.set_fill_color(self.scrollbar_thumb_color);
				graphic.draw_rect(thumb_rect);
			}
		}
	}
}

impl PaneElement {
	//==============================================================================
	// absolute 요소의 자리와 크기. (부모 크기에 대고, left 와 right 가 다 있고 폭이 auto 면 그 사이를 채운다)
	//==============================================================================
	fn place_absolute(&self, parent_size: Vector2, intrinsic_size: Vector2) -> (Vector2, Vector2) {
		let resolve_axis = |start: Option<f32>, end: Option<f32>, length: PaneLength, minimum: PaneLength, maximum: PaneLength, parent_length: f32, intrinsic_length: f32| -> (f32, f32) {
			let mut size = match (length.resolve(parent_length), start, end) {
				(Some(size), _, _) => {
					size
				}
				(None, Some(start), Some(end)) => {
					(parent_length - start - end).max(0.0)
				}
				(None, _, _) => {
					intrinsic_length
				}
			};
			size = clamp_length(size, minimum.resolve(parent_length).unwrap_or(0.0), maximum.resolve(parent_length).unwrap_or(f32::INFINITY));
			let position = match (start, end) {
				(Some(start), _) => {
					start
				}
				(None, Some(end)) => {
					parent_length - end - size
				}
				(None, None) => {
					0.0
				}
			};
			(position, size)
		};
		let (x, width) = resolve_axis(self.left, self.right, self.width, self.minimum_width, self.maximum_width, parent_size.x, intrinsic_size.x);
		let (y, height) = resolve_axis(self.top, self.bottom, self.height, self.minimum_height, self.maximum_height, parent_size.y, intrinsic_size.y);
		(Vector2::create(x, y), Vector2::create(width, height))
	}

	//==============================================================================
	// 자식 배치. (absolute 는 부모 크기에, 그 밖은 안쪽 여백 안에서 블록(세로로 차례) 또는 flex 로. 넘침을 굴려 넘긴 만큼 옮긴다)
	//==============================================================================
	fn lay_out_children(&mut self, context: &mut ComponentContext) {
		let node_id = context.get_node_id();
		let node_size = context.get_node_tree()[node_id].get_content_size();
		let frame_size = self.read_frame_size();
		let content_left = self.padding[3] + self.border_width_list[3];
		let content_top = self.padding[0] + self.border_width_list[0];
		let content_width = (node_size.x - frame_size.x).max(0.0);
		let content_height = (node_size.y - frame_size.y).max(0.0);
		let child_id_list: Vec<NodeId> = context.get_node_tree().get_child_ids(node_id).to_vec();
		// (노드, 자리와 크기, 스크롤을 받는지)
		let mut placement_list: Vec<(NodeId, Option<(Vector2, Vector2)>, bool)> = Vec::new();
		let mut flow_y = content_top;
		let mut flex_item_list: Vec<PaneFlexItem> = Vec::new();
		let is_row = self.flex_direction == PaneFlexDirection::Row;
		let content_main = if is_row {
			content_width
		}
		else {
			content_height
		};
		let content_cross = if is_row {
			content_height
		}
		else {
			content_width
		};
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
				placement_list.push((child_id, None, false));
				continue;
			}
			let intrinsic_size = measure_element_size(context.get_node_tree(), child_id);
			if child_element.is_absolute {
				let (position, size) = child_element.place_absolute(node_size, intrinsic_size);
				placement_list.push((child_id, Some((position, size)), false));
				continue;
			}
			let margin = child_element.margin;
			if self.display == PaneDisplay::Flex {
				let (main_length, main_minimum, main_maximum, main_intrinsic, main_overflow) = if is_row {
					(child_element.width, child_element.minimum_width, child_element.maximum_width, intrinsic_size.x, child_element.overflow_x)
				}
				else {
					(child_element.height, child_element.minimum_height, child_element.maximum_height, intrinsic_size.y, child_element.overflow_y)
				};
				let (cross_length, cross_minimum, cross_maximum, cross_intrinsic) = if is_row {
					(child_element.height, child_element.minimum_height, child_element.maximum_height, intrinsic_size.y)
				}
				else {
					(child_element.width, child_element.minimum_width, child_element.maximum_width, intrinsic_size.x)
				};
				let main_margin = if is_row {
					(margin[3], margin[1])
				}
				else {
					(margin[0], margin[2])
				};
				let cross_margin = if is_row {
					(margin[0], margin[2])
				}
				else {
					(margin[3], margin[1])
				};
				// 기준 크기: flex-basis, 없으면 주축 크기, 그것도 auto 면 제 크기.
				let base_size = child_element.flex_basis.resolve(content_main).or_else(|| main_length.resolve(content_main)).unwrap_or(main_intrinsic);
				// 자동 최소: 넘침이 visible 인 항목은 제 크기(글이 줄바꿈하지 않아 min-content 와 같다), 스크롤 상자는 0.
				let automatic_minimum = if main_overflow == PaneOverflow::Visible {
					main_intrinsic.min(main_length.resolve(content_main).unwrap_or(f32::INFINITY))
				}
				else {
					0.0
				};
				let minimum_main = main_minimum.resolve(content_main).unwrap_or(automatic_minimum);
				let maximum_main = main_maximum.resolve(content_main).unwrap_or(f32::INFINITY);
				let cross_size = match cross_length.resolve(content_cross) {
					Some(cross_size) => {
						cross_size
					}
					None if self.align_items == PaneAlignItems::Stretch => {
						(content_cross - cross_margin.0 - cross_margin.1).max(0.0)
					}
					None => {
						cross_intrinsic
					}
				};
				let cross_size = clamp_length(cross_size, cross_minimum.resolve(content_cross).unwrap_or(0.0), cross_maximum.resolve(content_cross).unwrap_or(f32::INFINITY));
				flex_item_list.push(PaneFlexItem {
					node_id: child_id,
					main_margin,
					cross_margin,
					base_size,
					minimum_main,
					maximum_main,
					flex_grow: child_element.flex_grow,
					flex_shrink: child_element.flex_shrink,
					main_size: clamp_length(base_size, minimum_main, maximum_main),
					cross_size,
				});
			}
			else {
				let available_width = (content_width - margin[1] - margin[3]).max(0.0);
				let auto_width = if child_element.is_block_level() {
					available_width
				}
				else {
					intrinsic_size.x
				};
				let width = clamp_length(child_element.width.resolve(content_width).unwrap_or(auto_width), child_element.minimum_width.resolve(content_width).unwrap_or(0.0), child_element.maximum_width.resolve(content_width).unwrap_or(f32::INFINITY));
				let height = clamp_length(child_element.height.resolve(content_height).unwrap_or(intrinsic_size.y), child_element.minimum_height.resolve(content_height).unwrap_or(0.0), child_element.maximum_height.resolve(content_height).unwrap_or(f32::INFINITY));
				flow_y += margin[0];
				placement_list.push((child_id, Some((Vector2::create(content_left + margin[3], flow_y), Vector2::create(width, height))), true));
				flow_y += height + margin[2];
			}
		}
		if !flex_item_list.is_empty() {
			self.resolve_flexible_lengths(&mut flex_item_list, content_main);
			let used_main: f32 = flex_item_list.iter().map(|item| item.main_size + item.main_margin.0 + item.main_margin.1).sum::<f32>() + self.gap * (flex_item_list.len() as f32 - 1.0);
			let remaining_main = content_main - used_main;
			let item_count = flex_item_list.len() as f32;
			let (mut main_position, between_space) = match self.justify_content {
				PaneJustifyContent::Start => {
					(0.0, 0.0)
				}
				PaneJustifyContent::Center => {
					(remaining_main * 0.5, 0.0)
				}
				PaneJustifyContent::End => {
					(remaining_main, 0.0)
				}
				PaneJustifyContent::SpaceBetween => {
					if remaining_main > 0.0 && item_count > 1.0 {
						(0.0, remaining_main / (item_count - 1.0))
					}
					else {
						(0.0, 0.0)
					}
				}
				PaneJustifyContent::SpaceAround => {
					if remaining_main > 0.0 {
						(remaining_main / item_count * 0.5, remaining_main / item_count)
					}
					else {
						(remaining_main * 0.5, 0.0)
					}
				}
				PaneJustifyContent::SpaceEvenly => {
					if remaining_main > 0.0 {
						(remaining_main / (item_count + 1.0), remaining_main / (item_count + 1.0))
					}
					else {
						(remaining_main * 0.5, 0.0)
					}
				}
			};
			for item in flex_item_list.iter() {
				main_position += item.main_margin.0;
				let cross_position = match self.align_items {
					PaneAlignItems::Center => {
						(content_cross - item.cross_size - item.cross_margin.0 - item.cross_margin.1) * 0.5 + item.cross_margin.0
					}
					PaneAlignItems::End => {
						content_cross - item.cross_size - item.cross_margin.1
					}
					PaneAlignItems::Start | PaneAlignItems::Stretch => {
						item.cross_margin.0
					}
				};
				let (position, size) = if is_row {
					(Vector2::create(content_left + main_position, content_top + cross_position), Vector2::create(item.main_size, item.cross_size))
				}
				else {
					(Vector2::create(content_left + cross_position, content_top + main_position), Vector2::create(item.cross_size, item.main_size))
				};
				placement_list.push((item.node_id, Some((position, size)), true));
				main_position += item.main_size + item.main_margin.1 + self.gap + between_space;
			}
		}

		// 넘침: 내용 끝(안쪽 여백 상자 기준, 흐르는 자식의 바깥 끝 + 오른쪽과 아래 안쪽 여백)을 재고 굴려 넘긴 만큼을 줄인다.
		if self.is_clipping() {
			let mut extent = Vector2::create(content_width + self.padding[1] + self.padding[3], content_height + self.padding[0] + self.padding[2]);
			for (_, placement, is_scrolled) in placement_list.iter() {
				if let (Some((position, size)), true) = (placement, *is_scrolled) {
					extent.x = extent.x.max(position.x + size.x - self.border_width_list[3] + self.padding[1]);
					extent.y = extent.y.max(position.y + size.y - self.border_width_list[0] + self.padding[2]);
				}
			}
			self.scroll_extent = extent;
			let viewport_size = Vector2::create(node_size.x - self.border_width_list[1] - self.border_width_list[3], node_size.y - self.border_width_list[0] - self.border_width_list[2]);
			self.clamp_scroll_offset(viewport_size);
		}
		else {
			self.scroll_extent = Vector2::zero();
			self.scroll_offset = Vector2::zero();
		}
		let scroll_offset = self.scroll_offset;
		let node_tree = context.get_node_tree_mut();
		for (child_id, placement, is_scrolled) in placement_list {
			let child_node = &mut node_tree[child_id];
			match placement {
				Some((position, size)) => {
					let position = if is_scrolled {
						Vector2::create(position.x - scroll_offset.x, position.y - scroll_offset.y)
					}
					else {
						position
					};
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

	//==============================================================================
	// flex 길이 정하기. (CSS Flexbox 9.7: 남는 자리는 grow 비로 나누고, 모자라면 shrink 와 기준 크기를 곱한 비로 줄인다. min, max 에 걸린 항목은 묶고 다시 나눈다)
	//==============================================================================
	fn resolve_flexible_lengths(&self, item_list: &mut [PaneFlexItem], content_main: f32) {
		let gap_total = self.gap * (item_list.len() as f32 - 1.0);
		let read_outer = |item: &PaneFlexItem, size: f32| -> f32 {
			size + item.main_margin.0 + item.main_margin.1
		};
		// 1. 키울지 줄일지. (가정 크기 합과 견준다)
		let hypothetical_total: f32 = item_list.iter().map(|item| read_outer(item, item.main_size)).sum::<f32>() + gap_total;
		let is_growing = hypothetical_total < content_main;
		let read_factor = |item: &PaneFlexItem| -> f32 {
			if is_growing {
				item.flex_grow
			}
			else {
				item.flex_shrink
			}
		};
		// 2. 늘지 않는 항목은 가정 크기로 묶는다.
		let mut is_frozen_list: Vec<bool> = item_list.iter().map(|item| read_factor(item) == 0.0 || (is_growing && item.base_size > item.main_size) || (!is_growing && item.base_size < item.main_size)).collect();
		let read_remaining_space = |item_list: &[PaneFlexItem], is_frozen_list: &[bool]| -> f32 {
			let used_total: f32 = item_list
				.iter()
				.zip(is_frozen_list.iter())
				.map(|(item, is_frozen)| {
					if *is_frozen {
						read_outer(item, item.main_size)
					}
					else {
						read_outer(item, item.base_size)
					}
				})
				.sum::<f32>() + gap_total;
			content_main - used_total
		};
		// 3. 처음 남는 자리.
		let initial_free_space = read_remaining_space(item_list, &is_frozen_list);
		// 4. 모두 묶일 때까지 나눈다.
		for _ in 0..=item_list.len() {
			if is_frozen_list.iter().all(|is_frozen| *is_frozen) {
				break;
			}
			let mut remaining_space = read_remaining_space(item_list, &is_frozen_list);
			let factor_total: f32 = item_list.iter().zip(is_frozen_list.iter()).filter(|(_, is_frozen)| !**is_frozen).map(|(item, _)| read_factor(item)).sum();
			if factor_total < 1.0 {
				let limited_space = initial_free_space * factor_total;
				if limited_space.abs() < remaining_space.abs() {
					remaining_space = limited_space;
				}
			}
			let scaled_shrink_total: f32 = item_list.iter().zip(is_frozen_list.iter()).filter(|(_, is_frozen)| !**is_frozen).map(|(item, _)| item.flex_shrink * item.base_size).sum();
			let mut violation_list: Vec<f32> = vec![0.0; item_list.len()];
			let mut total_violation = 0.0;
			for (item_index, item) in item_list.iter_mut().enumerate() {
				if is_frozen_list[item_index] {
					continue;
				}
				let target_size = if remaining_space == 0.0 {
					item.base_size
				}
				else if is_growing {
					item.base_size + remaining_space * item.flex_grow / factor_total
				}
				else if scaled_shrink_total > 0.0 {
					item.base_size + remaining_space * item.flex_shrink * item.base_size / scaled_shrink_total
				}
				else {
					item.base_size
				};
				let clamped_size = clamp_length(target_size, item.minimum_main, item.maximum_main).max(0.0);
				violation_list[item_index] = clamped_size - target_size;
				total_violation += clamped_size - target_size;
				item.main_size = clamped_size;
			}
			// 걸린 쪽의 항목을 묶는다. (걸림이 없으면 모두)
			for item_index in 0..item_list.len() {
				if is_frozen_list[item_index] {
					continue;
				}
				let violation = violation_list[item_index];
				is_frozen_list[item_index] = total_violation == 0.0 || (total_violation > 0.0 && violation > 0.0) || (total_violation < 0.0 && violation < 0.0);
			}
		}
	}

	//==============================================================================
	// 테두리. (네 쪽이 같으면 둥근 사각형 선, 아니면 쪽마다 띠)
	//==============================================================================
	fn draw_border(&self, content_size: Vector2, graphic: &mut Graphic) {
		let is_uniform = self.border_width_list.iter().all(|width| *width == self.border_width_list[0]) && self.border_color_list.iter().all(|color| *color == self.border_color_list[0]);
		if is_uniform {
			if let (Some(border_color), true) = (self.border_color_list[0], self.border_width_list[0] > 0.0) {
				let border_width = self.border_width_list[0];
				let half_width = border_width * 0.5;
				graphic.set_stroke_color(border_color);
				graphic.draw_stroke_round_rect(Rect::create(half_width, half_width, content_size.x - border_width, content_size.y - border_width), (self.border_radius - half_width).max(0.0), border_width);
			}
			return;
		}
		let side_rect_list = [
			Rect::create(0.0, 0.0, content_size.x, self.border_width_list[0]),
			Rect::create(content_size.x - self.border_width_list[1], 0.0, self.border_width_list[1], content_size.y),
			Rect::create(0.0, content_size.y - self.border_width_list[2], content_size.x, self.border_width_list[2]),
			Rect::create(0.0, 0.0, self.border_width_list[3], content_size.y),
		];
		for (side_index, side_rect) in side_rect_list.iter().enumerate() {
			if let (Some(border_color), true) = (self.border_color_list[side_index], self.border_width_list[side_index] > 0.0) {
				graphic.set_fill_color(border_color);
				graphic.draw_rect(*side_rect);
			}
		}
	}
}

//==============================================================================
// 화면 자리 → 노드 좌표. (거꿀 행렬이 없으면 None)
//==============================================================================
fn convert_view_to_local(node_tree: &NodeTree, node_id: NodeId, view_position: Vector2) -> Option<Vector2> {
	let world_matrix = node_tree.calculate_world_matrix(node_id);
	let inverse_matrix = world_matrix.invert()?;
	let local_point = inverse_matrix.transform_point(view_position.x, view_position.y);
	Some(Vector2::create(local_point.x, local_point.y))
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

	//==============================================================================
	// 원본 에디터의 꼴: 세로 flex 패널(제목 + flex 1 의 스크롤 목록), 가로 flex 머리줄(flex 1 의 빈칸이 오른쪽 아이콘을 민다). 휠과 끌기로 목록을 넘긴다.
	//==============================================================================
	#[test]
	fn pane_style_reads_flex_and_overflow() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(400.0, 300.0));
		let holder_id = node_tree.create_child(root_id, "holder");
		{
			let holder_node = &mut node_tree[holder_id];
			holder_node.set_pivot(Pivot::TOP_LEFT);
			holder_node.set_anchor(Vector2::zero());
			holder_node.set_content_size(Vector2::create(240.0, 300.0));
		}
		let section_id = PaneStyle::create(&mut node_tree, "div", "panel", &PaneStyleOptions { style: &[("display", "flex"), ("flex-direction", "column"), ("padding", "0")], ..Default::default() });
		node_tree.add_child(holder_id, section_id);
		let header_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("position", "relative"), ("width", "auto"), ("height", "30px"), ("display", "flex"), ("align-items", "center"), ("padding", "0 8px"), ("flex-shrink", "0"), ("border-bottom", "1px solid #444")], ..Default::default() });
		let title_id = PaneStyle::create(&mut node_tree, "span", "", &PaneStyleOptions { text: "HIERARCHY", style: &[("position", "relative"), ("width", "auto"), ("height", "auto")], ..Default::default() });
		let spacer_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("position", "relative"), ("width", "auto"), ("height", "auto"), ("flex", "1")], ..Default::default() });
		let more_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { text: "...", style: &[("position", "relative"), ("width", "20px"), ("height", "16px")], ..Default::default() });
		node_tree.add_child(section_id, header_id);
		node_tree.add_child(header_id, title_id);
		node_tree.add_child(header_id, spacer_id);
		node_tree.add_child(header_id, more_id);
		let list_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("flex", "1"), ("overflow-y", "auto"), ("position", "relative")], ..Default::default() });
		node_tree.add_child(section_id, list_id);
		let mut row_id_list: Vec<NodeId> = Vec::new();
		for row_index in 0..20 {
			let row_text = format!("row {}", row_index);
			let row_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { text: &row_text, style: &[("position", "relative"), ("width", "auto"), ("height", "24px"), ("display", "flex"), ("align-items", "center")], ..Default::default() });
			node_tree.add_child(list_id, row_id);
			row_id_list.push(row_id);
		}
		for _ in 0..3 {
			node_tree.tick(&engine, 1.0 / 60.0);
		}
		// 세로 flex: 머리줄은 30 그대로, 목록이 남은 270 을 채우고 폭은 늘어난다. (align-items 기본 stretch)
		assert_eq!(node_tree[header_id].get_local_position(), Vector2::create(0.0, 0.0));
		assert_eq!(node_tree[header_id].get_content_size(), Vector2::create(240.0, 30.0));
		assert_eq!(node_tree[list_id].get_local_position(), Vector2::create(0.0, 30.0));
		assert_eq!(node_tree[list_id].get_content_size(), Vector2::create(240.0, 270.0));
		// 가로 flex: 빈칸이 남은 폭을 먹어 아이콘이 오른쪽 안쪽 여백 8 앞에 붙고, 세로는 가운데. (아래 테두리 1 을 뺀 29 의 가운데)
		let more_position = node_tree[more_id].get_local_position();
		assert!((more_position.x - (240.0 - 8.0 - 20.0)).abs() < 0.01);
		assert!((more_position.y - (29.0 - 16.0) * 0.5).abs() < 0.01);
		let title_size = node_tree[title_id].get_content_size();
		let spacer_size = node_tree[spacer_id].get_content_size();
		assert!((8.0 + title_size.x + spacer_size.x - more_position.x).abs() < 0.01);
		// 목록: 줄 20 개 = 480, 휠로 넘기면 줄이 위로 가고, 끝을 넘지 않는다.
		assert_eq!(node_tree[row_id_list[1]].get_local_position(), Vector2::create(0.0, 24.0));
		let list_center = Vector2::create(120.0, 30.0 + 135.0);
		assert_eq!(node_tree.find_wheel_target_node(list_center), Some(list_id));
		assert!(node_tree.dispatch_wheel_at(list_center, &engine, Vector2::create(0.0, 100.0)));
		assert_eq!(node_tree[row_id_list[1]].get_local_position(), Vector2::create(0.0, 24.0 - 100.0));
		assert!(node_tree.dispatch_wheel_at(list_center, &engine, Vector2::create(0.0, 1000.0)));
		assert_eq!(node_tree[list_id].get_component::<PaneElement>().expect("list").get_scroll_offset(), Vector2::create(0.0, 480.0 - 270.0));
		// 끝에 닿으면 더 받지 않고 부모에게 넘긴다.
		assert!(!node_tree.dispatch_wheel_at(list_center, &engine, Vector2::create(0.0, 10.0)));
		// 썸 끌기: 맨 위로.
		let thumb_rect = node_tree[list_id].get_component::<PaneElement>().expect("list").calculate_scrollbar_thumb_rect(Vector2::create(240.0, 270.0), true).expect("thumb");
		let thumb_center = Vector2::create(thumb_rect.position.x + thumb_rect.size.x * 0.5, 30.0 + thumb_rect.position.y + thumb_rect.size.y * 0.5);
		node_tree.dispatch_touch(list_id, &engine, TouchKind::Press, thumb_center);
		node_tree.dispatch_touch(list_id, &engine, TouchKind::Moved, Vector2::create(thumb_center.x, thumb_center.y - 400.0));
		node_tree.dispatch_touch(list_id, &engine, TouchKind::Release, Vector2::create(thumb_center.x, thumb_center.y - 400.0));
		assert_eq!(node_tree[list_id].get_component::<PaneElement>().expect("list").get_scroll_offset(), Vector2::zero());
		assert_eq!(node_tree[row_id_list[1]].get_local_position(), Vector2::create(0.0, 24.0));
	}

	//==============================================================================
	// flex 길이 셈: grow 비, shrink 와 기준 크기의 비, max 에 걸린 항목은 묶고 나머지에 다시 나눈다. justify-content space-between.
	//==============================================================================
	#[test]
	fn pane_flex_lengths_follow_css() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(300.0, 100.0));
		let holder_id = node_tree.create_child(root_id, "holder");
		{
			let holder_node = &mut node_tree[holder_id];
			holder_node.set_pivot(Pivot::TOP_LEFT);
			holder_node.set_anchor(Vector2::zero());
			holder_node.set_content_size(Vector2::create(300.0, 100.0));
		}
		let row_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("display", "flex"), ("gap", "10px")], ..Default::default() });
		node_tree.add_child(holder_id, row_id);
		let create_item = |node_tree: &mut NodeTree, style: &[(&str, &str)]| -> NodeId {
			let item_id = PaneStyle::create(node_tree, "div", "", &PaneStyleOptions { style, ..Default::default() });
			node_tree.add_child(row_id, item_id);
			item_id
		};
		let first_id = create_item(&mut node_tree, &[("position", "relative"), ("width", "40px"), ("height", "auto"), ("flex", "1")]);
		let second_id = create_item(&mut node_tree, &[("position", "relative"), ("width", "40px"), ("height", "auto"), ("flex-grow", "2"), ("max-width", "100px")]);
		let third_id = create_item(&mut node_tree, &[("position", "relative"), ("width", "50px"), ("height", "auto")]);
		for _ in 0..3 {
			node_tree.tick(&engine, 1.0 / 60.0);
		}
		// 첫째는 flex 1 이라 기준 0, 둘째는 기준 40 에 grow 2 (max 100), 셋째는 50 그대로. 남는 300 - 20 - 0 - 40 - 50 = 190.
		// 처음 나눔: 첫째 63.3, 둘째 40 + 126.7 = 166.7 → max 100 에 걸려 묶인다. 다시 나눔: 첫째 = 300 - 20 - 100 - 50 = 130.
		assert!((node_tree[first_id].get_content_size().x - 130.0).abs() < 0.01);
		assert!((node_tree[second_id].get_content_size().x - 100.0).abs() < 0.01);
		assert!((node_tree[third_id].get_content_size().x - 50.0).abs() < 0.01);
		assert!((node_tree[third_id].get_local_position().x - 250.0).abs() < 0.01);
		// 줄이기: 폭 합이 넘치면 shrink 1 과 기준 크기의 비로 줄인다. (제 크기 0 인 빈 칸이라 자동 최소도 0)
		PaneStyle::apply(&mut node_tree, first_id, &[("flex", "0 1 200px")]);
		PaneStyle::apply(&mut node_tree, second_id, &[("flex", "0 1 100px"), ("max-width", "none")]);
		PaneStyle::apply(&mut node_tree, third_id, &[("flex-shrink", "0"), ("width", "100px")]);
		node_tree.tick(&engine, 1.0 / 60.0);
		// 넘침 = 200 + 100 + 100 + 20 - 300 = 120, 첫째가 2/3, 둘째가 1/3 을 줄인다.
		assert!((node_tree[first_id].get_content_size().x - 120.0).abs() < 0.01);
		assert!((node_tree[second_id].get_content_size().x - 60.0).abs() < 0.01);
		assert!((node_tree[third_id].get_content_size().x - 100.0).abs() < 0.01);
		// space-between: 남는 자리를 사이에 나눈다.
		PaneStyle::apply(&mut node_tree, row_id, &[("justify-content", "space-between"), ("gap", "0")]);
		PaneStyle::apply(&mut node_tree, first_id, &[("flex", "none"), ("width", "50px")]);
		PaneStyle::apply(&mut node_tree, second_id, &[("flex", "none"), ("width", "50px")]);
		PaneStyle::apply(&mut node_tree, third_id, &[("width", "50px")]);
		node_tree.tick(&engine, 1.0 / 60.0);
		assert!((node_tree[second_id].get_local_position().x - 125.0).abs() < 0.01);
		assert!((node_tree[third_id].get_local_position().x - 250.0).abs() < 0.01);
	}
}
