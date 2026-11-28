//==============================================================================
//! 엔진 노드 화면 분할 패널. (원본 web/pane.js 의 PaneStyle, Pane 을 데스크탑에서 노드로 짓는 판, 공통 형과 셈은 web/pane.rs)
//! - 원본의 DOM 요소 자리가 노드다. 컨테이너는 노드(get_container)이고, 내용은 node_tree.add_child 로 그 자식에 붙인다. (원본 appendChild)
//!   데스크탑에는 전역 document 가 없어 노드를 만들고 고치는 함수는 NodeTree 를 받는다. 길이 단위는 원본의 CSS 픽셀 자리인 노드 좌표다.
//!   ```text
//!     let main_pane = Pane::new(node_tree, PaneOptions { direction: PaneDirection::Horizontal, ..Default::default() });
//!     let left_pane = Pane::new(node_tree, PaneOptions { size: PaneSize::Fixed(240.0), min_size: 170.0, ..Default::default() });
//!     main_pane.add_pane(node_tree, &left_pane);
//!     let panel_id = PaneStyle::create(node_tree, "div", "panel", &PaneStyleOptions::default());
//!     node_tree.add_child(left_pane.get_container(), panel_id);
//!     main_pane.attach_to(node_tree, root_id);
//!   ```
//! - 컨테이너는 원본처럼 바탕을 COLOR_BACKGROUND 로 칠하고 넘치는 자식을 자른다. (overflow: hidden, Mask)
//! - 리사이저는 원본처럼 끌 수 있으면 두께 5, 아니면 1 의 선이고 COLOR_RESIZER 로, 올려 두거나 끄는 동안 COLOR_RESIZER_HOVER 로 칠한다.
//!   눌러 끌면 잡은 자리와 경계 사이 간격을 지켜 앞이나 뒤 자식의 고정 크기를 바꾸고 뿌리부터 다시 놓는다. 올려 두거나 끄는 동안 col-resize, row-resize 커서다.
//! - attach_to 한 뿌리 패널은 부모 노드 크기가 바뀌면 다시 놓는다. (원본 창 resize 리스너와 다음 프레임의 refresh)
//! - PaneStyle::create 는 원본 프리셋과 같은 모양의 노드를 짓는다. (PaneElement 컴포넌트)
//!   공통: 왼쪽 위에 놓고 부모 크기를 채운다(position absolute, 100%), 글꼴 13px sans-serif, 글 색 COLOR_TEXT.
//!   panel: 바탕 COLOR_PANEL, 안쪽 여백 10. toolbar: 바탕 COLOR_TOOLBAR, 아래 테두리 1px COLOR_BORDER, 가로로 가운데 모아 10 씩 띄운다(flex).
//!   button: 제 크기(position relative, auto), 안쪽 여백 6 16, 모서리 4, 굵은 글, 바탕 COLOR_RESIZER(accent 면 COLOR_ACCENT, warning 이면 COLOR_WARNING),
//!   올려 두면 밝기 1.2 배와 pointer 커서, 누르고 떼면 set_on_click 의 함수. input: 엔진 입력 필드(UIInputField), 바탕 COLOR_INPUT_BACKGROUND, 안쪽 여백 4.
//!   textarea: 바탕 COLOR_BACKGROUND, 글 #d4d4d4, 고정폭 14px, 안쪽 여백 15, 탭 크기 4 의 여러 줄 입력 필드. text: 제 크기, 굵은 글.
//!   relative 요소는 부모 안쪽 여백 안에서 놓는다. 블록 부모는 세로로 차례로(div 는 폭을 채우고 그 밖은 제 폭), flex 부모는 CSS Flexbox 처럼
//!   주축(flex-direction row, column)으로 기준 크기(flex-basis, 없으면 width 나 height, 그것도 auto 면 제 크기)를 놓고 남는 자리를 flex-grow 비로 나누거나
//!   모자라면 flex-shrink 와 기준 크기를 곱한 비로 줄인다. (min, max 에 걸린 항목은 묶고 다시 나눈다, 넘침이 visible 인 항목의 자동 최소는 제 크기)
//!   교차축은 align-items(기본 stretch, flex-start, center, flex-end), 주축 늘어놓기는 justify-content(flex-start, center, flex-end, space-between, space-around, space-evenly)와 gap(줄 사이, 칸 사이).
//!   flex-wrap(wrap, wrap-reverse)이면 CSS Flexbox 9.3 처럼 바깥 가정 주축 크기를 차례로 더하다 넘치는 항목에서 새 줄을 열고, 줄마다 늘고 줄이며,
//!   줄의 교차 크기는 항목 바깥 교차 크기의 가장 큰 값이고 align-content(기본 stretch 는 남는 자리를 줄마다 고루 더한다)로 줄을 늘어놓는다. wrap-reverse 는 교차축의 시작과 끝을 바꾼다.
//!   auto 크기 요소의 제 크기는 글과 자식(블록은 세로 합, 가로 flex 는 가로 합)에 안쪽 여백과 테두리를 더한 것이다.
//! - overflow(overflow-x, overflow-y)가 visible 이 아니면 넘친 자식을 자르고(모서리 border-radius), auto, scroll 이면 휠로 넘기고 썸을 끌어 넘긴다.
//!   원본 스크롤 상자처럼 그 쪽으로 더 넘길 수 없으면 휠을 부모에게 넘긴다. 썸은 scrollbar-color 의 썸 색(원본 에디터의 rgba(121, 121, 121, 0.4))이다.
//! - 원본과 다른 점:
//!   - 원본 요소에 addEventListener("click") 으로 다는 것은 PaneElement::set_on_click 이다.
//!   - options.style 과 PaneStyle::apply 가 읽는 CSS 는 프리셋과 원본 도구(파티클, 타임라인, UI 에디터)가 쓰는 것이다:
//!     position(absolute, relative), left, top, right, bottom, width, height, min-width, min-height, max-width, max-height(px, %, auto),
//!     display(flex, block, none), flex-direction, flex, flex-grow, flex-shrink, flex-basis, justify-content, align-items, gap,
//!     padding, margin(1 ~ 4 값과 쪽마다), background, background-color, color, font-size, font-weight, font-family, line-height, text-align,
//!     border, border-top, border-right, border-bottom, border-left, border-width, border-color, border-radius, cursor(pointer),
//!     overflow, overflow-x, overflow-y, scrollbar-color, scrollbar-width, flex-wrap, flex-flow, align-content, row-gap, column-gap, white-space, text-overflow, opacity.
//!     opacity 는 크롬처럼 요소와 자손을 묶어 한 번에 얹는다. (노드 투명도와 투명도 층, WorldNode::set_opacity_layer)
//!     그 밖(user-select 같은 것)은 읽지 않는다. 글은 줄바꿈하지 않는다. (white-space: nowrap 과 text-overflow: ellipsis 에 overflow 가 visible 이 아니면 넘친 끝을 말줄임표로)
//!   - 스크롤 막대는 크롬처럼 운영체제를 따른다. 윈도우와 리눅스는 자리를 차지하는 Fluent 막대(두께 15, thin 10), 맥은 시스템 설정이 얹는 막대면 두께 8 의 얹는 썸,
//!     클래식이면 자리를 차지하는 막대(두께 15, thin 11)다. 자리를 차지하는 막대는 테두리 안, 안쪽 여백 밖에서 overflow: scroll 은 늘, auto 는 넘칠 때만 내용 폭을 던다.
//!     (web/pane.rs 의 read_classic_scrollbar_metrics, 단추 화살표는 그리지 않고 길은 scrollbar-color 의 둘째 값이 있을 때만 칠한다)
//!   - textarea 태그는 엔진 여러 줄 입력 필드(UIInputField::set_multiline)가 글을 받는다. 원본 DOM textarea 처럼 줄을 넣고, 폭에 맞춰 나누고, 위아래로 옮기고, 넘치면 휠로 넘긴다.
//!     프리셋의 탭 크기 4 를 쓴다. 글자, 색, 안쪽 여백은 만들 때의 스타일을 받는다. (PaneStyle::apply 로 나중에 바꾼 글꼴은 필드에 옮기지 않는다)
//!   - canvas 태그는 원본에서 미리보기 엔진의 캔버스가 들어가는 자리다. 데스크탑은 미리보기 장면을 렌더 타겟에 그려 PaneElement::set_canvas_texture 로 넣으면
//!     그 요소의 안쪽 상자에 그 화면을 그린다. (상자 크기는 calculate_canvas_box_size, 기기 픽셀 비를 곱해 렌더 타겟 크기를 맞춘다) 넣지 않으면 바탕과 테두리만 그린다.
//==============================================================================
use super::{calculate_pane_layout, calculate_resized_size, calculate_resizer_thickness, PaneDirection, PaneLayoutChild, PaneOptions, PaneResizableEdges, PaneResizeCallback, PaneSize, PaneStyleOptions, PaneTheme};
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::mask::Mask;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{GlTexture, Graphic, TextAlign, TextBaseline, FLOATS_PER_VERTEX};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::core::textstringtexturecache::measure_text_without_graphic;
use crate::ui::uiinputfield::UIInputField;
use crate::ui::uiview::is_view_position_inside_node;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// 단추를 누르고 떼면 부르는 함수. (원본 click 리스너)
pub type PaneClickCallback = Box<dyn FnMut(&mut ComponentContext)>;

//==============================================================================
/// 자식 사이 리사이저 하나.
//==============================================================================
#[derive(Clone, Copy)]
struct PaneResizer {
	node_id: NodeId,
	thickness: f64,
}

//==============================================================================
/// 패널 상태. (Pane 손잡이들과 컨테이너, 리사이저 노드의 컴포넌트가 나눠 든다)
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
	/// 마지막으로 놓인 자리. ([x, y, 폭, 높이], 부모 컨테이너 기준)
	layout_rect: [f64; 4],
	/// attach_to 한 뿌리인지와 그때 부모 크기. (바뀌면 다시 놓는다)
	is_attached: bool,
	attached_parent_size: Option<Vector2>,
	/// 리사이저 끌기.
	active_resizer_index: Option<usize>,
	resize_grab_offset: f64,
}

//==============================================================================
/// 화면 분할 패널. (원본 Pane, clone 은 같은 패널을 가리킨다)
///
/// # 예제
/// 왼쪽 목록과 오른쪽 화면으로 나누어 뿌리 노드에 붙이고, 목록을 숨기기
/// ```rust
/// use vanilla::{NodeTree, Pane, PaneOptions, PaneSize, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// node_tree[root_id].set_content_size(Vector2::create(1000.0, 600.0));
/// let main_pane = Pane::new(&mut node_tree, PaneOptions::default());
/// let left_pane = Pane::new(&mut node_tree, PaneOptions { size: PaneSize::Fixed(240.0), min_size: 170.0, ..Default::default() });
/// let view_pane = Pane::new(&mut node_tree, PaneOptions { min_size: 200.0, ..Default::default() });
/// main_pane.add_pane(&mut node_tree, &left_pane);
/// main_pane.add_pane(&mut node_tree, &view_pane);
/// main_pane.attach_to(&mut node_tree, root_id);
/// assert_eq!(node_tree[left_pane.get_container()].get_content_size(), Vector2::create(240.0, 600.0));
/// assert_eq!(node_tree[view_pane.get_container()].get_local_position(), Vector2::create(245.0, 0.0));
/// left_pane.set_visible(&mut node_tree, false);
/// assert_eq!(node_tree[view_pane.get_container()].get_local_position(), Vector2::create(0.0, 0.0));
/// assert_eq!(node_tree[view_pane.get_container()].get_content_size(), Vector2::create(1000.0, 600.0));
/// ```
//==============================================================================
#[derive(Clone)]
pub struct Pane {
	state: Rc<RefCell<PaneState>>,
}

impl Pane {
	//==============================================================================
	/// 생성. (떨어진 컨테이너 노드를 만든다. 바탕과 자르기)
	///
	/// # 인자
	/// - `node_tree`: 컨테이너 노드를 만들 트리.
	/// - `options`: 패널 옵션.
	///
	/// # 돌려줌
	/// 새 패널. (컨테이너 노드는 아직 어디에도 붙지 않았다, attach_to 나 부모의 add_pane 으로 붙인다)
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
	/// 자식 추가. (앞 자식이 있으면 그 사이에 리사이저 노드를 둔다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `pane`: 붙일 자식 패널. (그 컨테이너 노드가 이 패널 컨테이너의 자식이 된다)
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
	/// 배치. (자기 자리를 놓고, 같은 셈으로 자식과 리사이저를 놓는다. 원본 updateLayout)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `x`: 컨테이너 왼쪽. (부모 노드 좌표)
	/// - `y`: 컨테이너 위. (부모 노드 좌표)
	/// - `width`: 컨테이너 폭.
	/// - `height`: 컨테이너 높이.
	//==============================================================================
	pub fn update_layout(&self, node_tree: &mut NodeTree, x: f64, y: f64, width: f64, height: f64) {
		let (container_id, direction, children, resizer_list) = {
			let mut state = self.state.borrow_mut();
			state.layout_rect = [x, y, width, height];
			(state.container_id, state.direction, state.children.clone(), state.resizer_list.clone())
		};
		if let Some(container_node) = node_tree.get_node_mut(container_id) {
			container_node.set_local_position(Vector2::create(x, y));
			container_node.set_content_size(Vector2::create(width, height));
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
						resizer_node.set_local_position(Vector2::create(resizer_rect[0], resizer_rect[1]));
						resizer_node.set_content_size(Vector2::create(resizer_rect[2], resizer_rect[3]));
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
	/// 배치 콜백 부르기. (콜백 안에서 이 패널을 다시 불러도 되게 빼 두었다가 돌려놓는다)
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
	/// 배치 초기화. (끌어서 바꾼 크기를 지운다, 자식까지)
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
	/// 부모 노드 크기에 맞춰 다시 놓는다. (부모가 없으면 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
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
		self.update_layout(node_tree, 0.0, 0.0, parent_size.x, parent_size.y);
	}

	//==============================================================================
	/// 노드에 단다. (그 자리에서 놓고, 뒤로 부모 크기가 바뀌면 다시 놓는다. 원본 attachTo 의 창 resize 리스너)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `parent_id`: 붙일 부모 노드.
	//==============================================================================
	pub fn attach_to(&self, node_tree: &mut NodeTree, parent_id: NodeId) {
		node_tree.add_child(parent_id, self.get_container());
		self.state.borrow_mut().is_attached = true;
		self.refresh(node_tree);
	}

	//==============================================================================
	/// 보이기 / 숨기기. (숨긴 패널은 자리를 차지하지 않고, 맞닿은 리사이저도 숨는다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `is_visible`: 보일지.
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
	/// 뿌리 패널. (부모를 따라 끝까지 올라간다)
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
	/// 보이는지.
	///
	/// # 돌려줌
	/// 보이면 true.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.state.borrow().is_visible
	}

	//==============================================================================
	/// flex 인지. (처음 크기가 Flex)
	///
	/// # 돌려줌
	/// 처음 크기가 Flex 면 true.
	//==============================================================================
	pub fn is_flex(&self) -> bool {
		self.state.borrow().initial_size == PaneSize::Flex
	}

	//==============================================================================
	/// 처음 크기.
	///
	/// # 돌려줌
	/// 만들 때 받은 처음 크기. (PaneOptions::size)
	//==============================================================================
	pub fn get_initial_size(&self) -> PaneSize {
		self.state.borrow().initial_size
	}

	//==============================================================================
	/// 고정 크기. (픽셀, 0 이면 처음 크기로 잰다)
	///
	/// # 돌려줌
	/// 고정 크기. (픽셀, 끌어서 정했거나 처음 크기가 Fixed 면 그 값, reset_layout 뒤나 그 밖은 0)
	//==============================================================================
	pub fn get_fixed_size(&self) -> f64 {
		self.state.borrow().fixed_size
	}

	//==============================================================================
	/// 고정 크기 설정. (다음 배치부터 쓴다. flex 패널에는 쓰이지 않는다)
	///
	/// # 인자
	/// - `fixed_size`: 새 고정 크기. (픽셀, 0 이면 처음 크기로 잰다)
	//==============================================================================
	pub fn set_fixed_size(&self, fixed_size: f64) {
		self.state.borrow_mut().fixed_size = fixed_size;
	}

	//==============================================================================
	/// 최소 크기.
	///
	/// # 돌려줌
	/// 가장 작은 크기. (픽셀)
	//==============================================================================
	pub fn get_min_size(&self) -> f64 {
		self.state.borrow().min_size
	}

	//==============================================================================
	/// 최대 크기.
	///
	/// # 돌려줌
	/// 가장 큰 크기. (픽셀)
	//==============================================================================
	pub fn get_max_size(&self) -> f64 {
		self.state.borrow().max_size
	}

	//==============================================================================
	/// 컨테이너 노드. (내용을 여기의 자식으로 붙인다)
	///
	/// # 돌려줌
	/// 컨테이너 노드.
	//==============================================================================
	pub fn get_container(&self) -> NodeId {
		self.state.borrow().container_id
	}

	//==============================================================================
	/// 배치 콜백 설정. (update_layout 이 끝날 때마다 부른다. None 이면 비운다)
	///
	/// # 인자
	/// - `resize_callback`: update_layout 이 끝날 때마다 부를 함수. (None 이면 비운다)
	//==============================================================================
	pub fn set_callback(&self, resize_callback: Option<PaneResizeCallback>) {
		self.state.borrow_mut().resize_callback = resize_callback;
	}

	//==============================================================================
	/// 리사이저로 끌 수 있는지.
	///
	/// # 돌려줌
	/// 리사이저로 끌 수 있으면 true.
	//==============================================================================
	pub fn is_resizable(&self) -> bool {
		self.state.borrow().is_resizable
	}

	//==============================================================================
	/// 끌 수 있는 가장자리.
	///
	/// # 돌려줌
	/// 끌 수 있는 가장자리.
	//==============================================================================
	pub fn get_resizable_edges(&self) -> PaneResizableEdges {
		self.state.borrow().resizable_edges
	}

	//==============================================================================
	/// 리사이저 끌기 시작. (잡은 자리와 경계 사이 간격을 기억해 두어야 처음 끌 때 크기가 튀지 않는다)
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
	/// 리사이저 끌기. (같은 셈으로 앞이나 뒤 자식의 고정 크기를 바꾸고 뿌리부터 다시 놓는다. 원본 onMouseMove)
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
	/// 리사이저 끌기 끝.
	//==============================================================================
	fn end_resize(&self) {
		self.state.borrow_mut().active_resizer_index = None;
	}
}

//==============================================================================
/// 컨테이너 노드의 컴포넌트. (바탕을 칠하고, attach_to 한 뿌리는 부모 크기가 바뀌면 다시 놓는다)
//==============================================================================
struct PaneContainer {
	component_base: ComponentBase,
	state: Weak<RefCell<PaneState>>,
}

#[crate::reflect]
impl PaneContainer {}

#[crate::reflect]
impl Component for PaneContainer {
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
	/// 컴포넌트 이름. ("PaneContainer")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"PaneContainer"
	}

	//==============================================================================
	/// 갱신. (attach_to 한 뿌리 패널은 부모 크기가 바뀌면 다시 놓는다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
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

	//==============================================================================
	/// 출력. (바탕을 COLOR_BACKGROUND 로 칠한다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		graphic.set_fill_color(PaneTheme::COLOR_BACKGROUND);
		graphic.draw_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y));
	}
}

//==============================================================================
/// 리사이저 노드의 컴포넌트. (올려 두면 색과 커서, 눌러 끌면 크기 바꾸기)
//==============================================================================
struct PaneResizerHandle {
	component_base: ComponentBase,
	state: Weak<RefCell<PaneState>>,
	resizer_index: usize,
	can_resize: bool,
	is_hovered: bool,
}

#[crate::reflect]
impl PaneResizerHandle {
	//==============================================================================
	/// 포인터의 부모 컨테이너 좌표. (나누는 방향 쪽)
	//==============================================================================
	fn read_pointer(&self, context: &ComponentContext, view_input_position: Vector2, direction: PaneDirection) -> Option<f64> {
		let node_id = context.get_node_id();
		let container_id = context.get_node_tree().get_parent_id(node_id)?;
		let local_position = context.get_node_tree().convert_view_position_to_node_position(container_id, view_input_position)?;
		Some(match direction {
			PaneDirection::Horizontal => {
				local_position.x
			}
			PaneDirection::Vertical => {
				local_position.y
			}
		})
	}

	//==============================================================================
	/// 끄는 중인지.
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

#[crate::reflect]
impl Component for PaneResizerHandle {
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
	/// 컴포넌트 이름. ("PaneResizerHandle")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"PaneResizerHandle"
	}

	//==============================================================================
	/// 트리에 붙음. (끌 수 있는 리사이저만 interactable)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		node_tree[node_id].set_interactable(self.can_resize);
	}

	//==============================================================================
	/// 갱신. (올려 두었는지 보고, 올려 두거나 끄는 동안 col-resize, row-resize 커서)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
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

	//==============================================================================
	/// 터치 누름. (끌기 시작)
	//==============================================================================
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

	//==============================================================================
	/// 터치 이동. (앞이나 뒤 자식의 고정 크기를 바꾸고 다시 놓는다)
	//==============================================================================
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

	//==============================================================================
	/// 터치 뗌. (끌기 끝)
	//==============================================================================
	fn touch_release(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		if let Some(state) = self.state.upgrade() {
			Pane { state }.end_resize();
		}
	}

	//==============================================================================
	/// 터치 취소. (끌기 끝)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		if let Some(state) = self.state.upgrade() {
			Pane { state }.end_resize();
		}
	}

	//==============================================================================
	/// 출력. (올려 두거나 끄는 동안 COLOR_RESIZER_HOVER, 아니면 COLOR_RESIZER 로 칠한다)
	//==============================================================================
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
/// 요소 표시. (원본 display)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneDisplay {
	Block,
	Flex,
	None,
}

//==============================================================================
/// 크기 값. (px, %, auto)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
enum PaneLength {
	Auto,
	Pixel(f64),
	Percent(f64),
}

impl PaneLength {
	//==============================================================================
	/// 길이 셈. (auto 면 None, %는 부모 길이에 대고)
	//==============================================================================
	fn resolve(self, parent_length: f64) -> Option<f64> {
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
/// flex 주축. (원본 flex-direction)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneFlexDirection {
	Row,
	Column,
}

//==============================================================================
/// 주축 늘어놓기. (원본 justify-content)
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
/// 줄넘김. (원본 flex-wrap, 기본 nowrap)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneFlexWrap {
	NoWrap,
	Wrap,
	/// 줄이 교차축 끝에서 시작으로 쌓인다.
	WrapReverse,
}

//==============================================================================
/// 여러 줄의 교차축 늘어놓기. (원본 align-content, 기본 normal 은 flex 상자에서 stretch)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneAlignContent {
	Stretch,
	Start,
	Center,
	End,
	SpaceBetween,
	SpaceAround,
	SpaceEvenly,
}

//==============================================================================
/// 교차축 맞추기. (원본 align-items, 기본 stretch)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneAlignItems {
	Stretch,
	Start,
	Center,
	End,
}

//==============================================================================
/// 넘침. (원본 overflow, overflow-x, overflow-y)
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
	/// 읽기. (clip 은 hidden 과 같이 자른다, 모르는 값은 visible)
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
	/// 굴려 넘길 수 있는지. (auto, scroll)
	//==============================================================================
	fn is_scrollable(self) -> bool {
		matches!(self, PaneOverflow::Auto | PaneOverflow::Scroll)
	}
}

//==============================================================================
/// 글 가로 맞춤. (원본 text-align)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneTextAlign {
	Left,
	Center,
	Right,
}

/// 스크롤 막대 두께와 가장 짧은 썸 길이. (얹는 막대)
const SCROLLBAR_THICKNESS: f64 = 8.0;
const SCROLLBAR_MINIMUM_THUMB_LENGTH: f64 = 20.0;
/// 스크롤 막대 썸 기본 색. (원본 에디터의 scrollbar-color 썸)
const SCROLLBAR_DEFAULT_THUMB_COLOR: &str = "rgba(121, 121, 121, 0.4)";

//==============================================================================
/// PaneStyle 이 지은 요소. (원본 DOM 요소의 스타일과 글, 블록과 flex 배치, 넘침 자르기와 스크롤, 단추 누르기)
///
/// # 예제
/// 패널 안의 단추를 누르고 떼면 함수 부르기
/// ```rust
/// use std::cell::Cell;
/// use std::rc::Rc;
/// use vanilla::{Engine, EngineConfiguration, NodeTree, PaneElement, PaneStyle, PaneStyleOptions, TouchKind, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// node_tree[root_id].set_content_size(Vector2::create(400.0, 300.0));
/// let panel_id = PaneStyle::create(&mut node_tree, "div", "panel", &PaneStyleOptions::default());
/// let button_id = PaneStyle::create(&mut node_tree, "button", "button", &PaneStyleOptions { text: "Save", ..Default::default() });
/// node_tree.add_child(root_id, panel_id);
/// node_tree.add_child(panel_id, button_id);
/// let click_count = Rc::new(Cell::new(0));
/// let click_count_for_callback = click_count.clone();
/// let button = node_tree[button_id].get_component_mut::<PaneElement>().unwrap();
/// assert_eq!(button.get_tag_name(), "button");
/// assert_eq!(button.get_text(), "Save");
/// button.set_on_click(Some(Box::new(move |_context| {
/// 	click_count_for_callback.set(click_count_for_callback.get() + 1);
/// })));
/// node_tree.tick(&engine, 1.0 / 60.0);
/// node_tree.tick(&engine, 1.0 / 60.0);
/// assert_eq!(node_tree[button_id].get_local_position(), Vector2::create(10.0, 10.0));
/// let button_size = node_tree[button_id].get_content_size();
/// let button_center = Vector2::create(10.0 + button_size.x * 0.5, 10.0 + button_size.y * 0.5);
/// node_tree.dispatch_touch(button_id, &engine, TouchKind::Press, button_center);
/// node_tree.dispatch_touch(button_id, &engine, TouchKind::Release, button_center);
/// assert_eq!(click_count.get(), 1);
/// ```
//==============================================================================
pub struct PaneElement {
	component_base: ComponentBase,
	tag_name: String,
	is_absolute: bool,
	/// 자리. (absolute 요소, None 이면 auto)
	left: Option<f64>,
	top: Option<f64>,
	right: Option<f64>,
	bottom: Option<f64>,
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
	flex_wrap: PaneFlexWrap,
	align_content: PaneAlignContent,
	/// 줄 사이와 칸 사이. (원본 gap 의 row-gap, column-gap)
	row_gap: f64,
	column_gap: f64,
	flex_grow: f64,
	flex_shrink: f64,
	flex_basis: PaneLength,
	/// 위, 오른쪽, 아래, 왼쪽.
	padding: [f64; 4],
	margin: [f64; 4],
	border_width_list: [f64; 4],
	border_color_list: [Option<Color>; 4],
	background_color: Option<Color>,
	border_radius: f64,
	color: Color,
	font_size: f64,
	is_bold: bool,
	font_family: String,
	line_height: Option<f64>,
	text_align: PaneTextAlign,
	/// 글을 줄바꿈하지 않는지와 넘치면 말줄임표를 그릴지. (원본 white-space: nowrap 과 text-overflow: ellipsis)
	is_nowrap_text: bool,
	is_text_ellipsis: bool,
	overflow_x: PaneOverflow,
	overflow_y: PaneOverflow,
	scrollbar_thumb_color: Color,
	/// 스크롤 막대 길 색과 얇은 막대인지. (원본 scrollbar-color 의 둘째 값, scrollbar-width: thin)
	scrollbar_track_color: Option<Color>,
	is_thin_scrollbar: bool,
	/// 클래식 스크롤 막대가 차지한 자리. (세로 막대의 폭, 가로 막대의 높이, 얹는 막대면 0)
	scrollbar_gutter: Vector2,
	/// 굴려 넘긴 만큼과 내용 끝. (안쪽 여백 상자 기준)
	scroll_offset: Vector2,
	scroll_extent: Vector2,
	scroll_drag_start: Option<(Vector2, Vector2, bool)>,
	text: String,
	is_button: bool,
	is_pointer_cursor: bool,
	/// CSS opacity. (준 적이 없으면 None, 노드 투명도와 투명도 층으로 옮긴다)
	opacity: Option<f64>,
	/// canvas 태그가 내용 상자에 그릴 렌더 타겟 색 텍스처. (원본은 그 자리에 미리보기 엔진의 캔버스가 들어간다)
	canvas_texture: Option<GlTexture>,
	is_hovered: bool,
	click_callback: Option<PaneClickCallback>,
}

#[crate::reflect]
impl PaneElement {
	//==============================================================================
	/// 생성. (원본 PaneStyle.create 의 공통 기본 스타일: absolute, 왼쪽 위, 100%, 13px sans-serif, COLOR_TEXT)
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
			flex_wrap: PaneFlexWrap::NoWrap,
			align_content: PaneAlignContent::Stretch,
			row_gap: 0.0,
			column_gap: 0.0,
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
			is_nowrap_text: false,
			is_text_ellipsis: false,
			overflow_x: PaneOverflow::Visible,
			overflow_y: PaneOverflow::Visible,
			scrollbar_thumb_color: Color::from(SCROLLBAR_DEFAULT_THUMB_COLOR),
			scrollbar_track_color: None,
			is_thin_scrollbar: false,
			scrollbar_gutter: Vector2::zero(),
			scroll_offset: Vector2::zero(),
			scroll_extent: Vector2::zero(),
			scroll_drag_start: None,
			text: String::new(),
			is_button: false,
			is_pointer_cursor: false,
			opacity: None,
			canvas_texture: None,
			is_hovered: false,
			click_callback: None,
		}
	}

	//==============================================================================
	/// canvas 태그가 그릴 화면 넣기. (원본 도구가 canvas 요소에 미리보기 엔진을 돌리는 자리, 데스크탑은 미리보기 장면을 그린 렌더 타겟의 색 텍스처를 넣는다)
	/// - canvas 태그 요소는 바탕과 테두리 위, 안쪽 여백과 테두리 안의 상자에 이 텍스처를 늘여 그린다. 렌더 타겟 텍스처처럼 아래가 0 행인 텍스처로 본다.
	/// - 렌더 타겟 크기는 원본 캔버스처럼 그 상자 크기에 기기 픽셀 비를 곱해 맞추면 1:1 로 그려진다. (calculate_canvas_box_size)
	///
	/// # 인자
	/// - `canvas_texture`: 그릴 색 텍스처. (None 이면 바탕과 테두리만 그린다)
	///
	/// # 예제
	/// 미리보기를 그린 렌더 타겟을 Pane 의 canvas 요소에 넣기
	/// ```rust,no_run
	/// use vanilla::{NodeId, NodeTree, PaneElement, RenderTarget};
	///
	/// fn show_preview(node_tree: &mut NodeTree, canvas_id: NodeId, render_target: &RenderTarget) {
	/// 	if let Some(element) = node_tree[canvas_id].get_component_mut::<PaneElement>() {
	/// 		element.set_canvas_texture(render_target.get_color_texture(0));
	/// 	}
	/// }
	/// ```
	//==============================================================================
	pub fn set_canvas_texture(&mut self, canvas_texture: Option<GlTexture>) {
		self.canvas_texture = canvas_texture;
	}

	//==============================================================================
	/// canvas 태그가 그릴 화면.
	///
	/// # 돌려줌
	/// set_canvas_texture 로 넣은 색 텍스처. (없으면 None)
	//==============================================================================
	pub fn get_canvas_texture(&self) -> Option<GlTexture> {
		self.canvas_texture
	}

	//==============================================================================
	/// canvas 태그가 화면을 그릴 상자 크기. (내용 크기에서 안쪽 여백과 테두리를 뺀 것, 노드 좌표이고 원본 canvas 요소의 clientWidth, clientHeight 자리)
	///
	/// # 인자
	/// - `content_size`: 요소 노드의 내용 크기. (node_tree[노드].get_content_size())
	///
	/// # 돌려줌
	/// 그릴 상자의 가로, 세로. (기기 픽셀 비를 곱하면 렌더 타겟 크기)
	//==============================================================================
	pub fn calculate_canvas_box_size(&self, content_size: Vector2) -> Vector2 {
		let frame_size = self.read_frame_size();
		Vector2::create((content_size.x - frame_size.x).max(0.0), (content_size.y - frame_size.y).max(0.0))
	}

	//==============================================================================
	/// 누르고 떼면 부를 함수. (원본 요소의 addEventListener("click"))
	///
	/// # 인자
	/// - `click_callback`: 누르고 떼면 부를 함수. (button 프리셋 요소만 부른다, None 이면 비운다)
	//==============================================================================
	pub fn set_on_click(&mut self, click_callback: Option<PaneClickCallback>) {
		self.click_callback = click_callback;
	}

	//==============================================================================
	/// 글 설정. (원본 innerText)
	///
	/// # 인자
	/// - `text`: 보일 글.
	//==============================================================================
	pub fn set_text(&mut self, text: &str) {
		self.text = text.to_string();
	}

	//==============================================================================
	/// 글.
	///
	/// # 돌려줌
	/// 보이는 글. (textarea 의 글은 입력 필드가 들어 여기는 비어 있다)
	//==============================================================================
	pub fn get_text(&self) -> &str {
		&self.text
	}

	//==============================================================================
	/// 태그 이름. (소문자)
	///
	/// # 돌려줌
	/// 소문자 태그 이름.
	//==============================================================================
	pub fn get_tag_name(&self) -> &str {
		&self.tag_name
	}

	//==============================================================================
	/// 굴려 넘긴 만큼. (원본 scrollLeft, scrollTop)
	///
	/// # 돌려줌
	/// 굴려 넘긴 만큼. (x 가 scrollLeft, y 가 scrollTop)
	//==============================================================================
	pub fn get_scroll_offset(&self) -> Vector2 {
		self.scroll_offset
	}

	//==============================================================================
	/// 굴려 넘기기. (원본 scrollLeft, scrollTop 넣기. 다음 배치에서 내용 끝 안으로 줄인다)
	///
	/// # 인자
	/// - `scroll_offset`: 굴려 넘길 만큼. (음수는 0 으로)
	//==============================================================================
	pub fn set_scroll_offset(&mut self, scroll_offset: Vector2) {
		self.scroll_offset = Vector2::create(scroll_offset.x.max(0.0), scroll_offset.y.max(0.0));
	}

	//==============================================================================
	/// CSS 속성 하나 넣기. (머리 주석의 속성만 읽는다, 그 밖은 무시)
	///
	/// # 인자
	/// - `property_name`: CSS 속성 이름. (background-color 같은 이름)
	/// - `property_value`: 값. (앞뒤 빈칸은 뗀다)
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
				// gap: 줄 사이 [칸 사이]. (값 하나면 둘 다)
				let mut part_list = value.split_whitespace();
				self.row_gap = parse_pixel(part_list.next().unwrap_or("")).unwrap_or(0.0);
				self.column_gap = part_list.next().and_then(parse_pixel).unwrap_or(self.row_gap);
			}
			"row-gap" => {
				self.row_gap = parse_pixel(value).unwrap_or(0.0);
			}
			"column-gap" => {
				self.column_gap = parse_pixel(value).unwrap_or(0.0);
			}
			"flex-wrap" => {
				self.flex_wrap = parse_flex_wrap(value);
			}
			"flex-flow" => {
				// flex-flow: 방향과 줄넘김을 차례 없이.
				for part in value.split_whitespace() {
					if part.starts_with("row") || part.starts_with("column") {
						self.apply_style("flex-direction", part);
					}
					else {
						self.flex_wrap = parse_flex_wrap(part);
					}
				}
			}
			"align-content" => {
				self.align_content = match value {
					"flex-start" | "start" => {
						PaneAlignContent::Start
					}
					"center" => {
						PaneAlignContent::Center
					}
					"flex-end" | "end" => {
						PaneAlignContent::End
					}
					"space-between" => {
						PaneAlignContent::SpaceBetween
					}
					"space-around" => {
						PaneAlignContent::SpaceAround
					}
					"space-evenly" => {
						PaneAlignContent::SpaceEvenly
					}
					_ => {
						PaneAlignContent::Stretch
					}
				};
			}
			"white-space" => {
				self.is_nowrap_text = matches!(value, "nowrap" | "pre");
			}
			"text-overflow" => {
				self.is_text_ellipsis = value == "ellipsis";
			}
			"scrollbar-width" => {
				self.is_thin_scrollbar = value == "thin";
			}
			"flex" => {
				self.apply_flex_shorthand(value);
			}
			"flex-grow" => {
				self.flex_grow = value.parse::<f64>().unwrap_or(0.0).max(0.0);
			}
			"flex-shrink" => {
				self.flex_shrink = value.parse::<f64>().unwrap_or(1.0).max(0.0);
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
					value.parse::<f64>().ok().map(|multiplier| multiplier * self.font_size)
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
			"opacity" => {
				self.opacity = parse_opacity(value);
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
				let color_text_list = split_css_values(value);
				if let Some(thumb_text) = color_text_list.first() {
					if *thumb_text != "auto" {
						self.scrollbar_thumb_color = Color::from(thumb_text.as_str());
					}
				}
				self.scrollbar_track_color = color_text_list.get(1).map(|track_text| Color::from(track_text.as_str()));
			}
			_ => {
			}
		}
	}

	//==============================================================================
	/// flex 줄임 꼴. (원본 flex: none, auto, 수 하나(grow, basis 0%), grow shrink, grow shrink basis, 길이 하나(basis))
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
		let mut number_list: Vec<f64> = Vec::new();
		let mut basis: Option<PaneLength> = None;
		for part in value.split_whitespace() {
			match part.parse::<f64>() {
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
	/// 한 축만 visible 이면 auto 로. (CSS: 다른 축이 visible 이 아니면 visible 은 auto 로 계산된다)
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
	/// 넘친 자식을 자르는지. (overflow 가 visible 이 아님)
	//==============================================================================
	fn is_clipping(&self) -> bool {
		self.overflow_x != PaneOverflow::Visible || self.overflow_y != PaneOverflow::Visible
	}

	//==============================================================================
	/// 폰트 문자열.
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
	/// 줄 높이. (line-height 가 없으면 CSS normal: 크롬 SimpleFontData 처럼 첫째 글꼴의 위아래 길이와 줄 사이로 셈한다. 글꼴을 읽지 못하면 글꼴 크기의 1.2 배)
	//==============================================================================
	fn read_line_height(&self) -> f64 {
		match self.line_height {
			Some(line_height) => {
				line_height
			}
			None => {
				crate::core::textstringtexturecache::measure_normal_line_height_without_graphic(&self.build_font_string()).unwrap_or_else(|| (self.font_size * 1.2).round())
			}
		}
	}

	//==============================================================================
	/// 글 크기. (글 폭과 줄 높이, 입력 칸은 원본 input 의 size 20 처럼 글자 스무 개 폭)
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
					self.text.chars().count() as f64 * self.font_size * 0.55
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
	/// 안쪽 여백과 테두리를 더한 둘레. (가로 합, 세로 합)
	//==============================================================================
	fn read_frame_size(&self) -> Vector2 {
		Vector2::create(self.padding[1] + self.padding[3] + self.border_width_list[1] + self.border_width_list[3], self.padding[0] + self.padding[2] + self.border_width_list[0] + self.border_width_list[2])
	}

	//==============================================================================
	/// 주축의 칸 사이. (가로 flex 는 column-gap, 세로는 row-gap)
	//==============================================================================
	fn read_main_gap(&self) -> f64 {
		if self.flex_direction == PaneFlexDirection::Row {
			self.column_gap
		}
		else {
			self.row_gap
		}
	}

	//==============================================================================
	/// 교차축의 줄 사이. (가로 flex 는 row-gap, 세로는 column-gap)
	//==============================================================================
	fn read_cross_gap(&self) -> f64 {
		if self.flex_direction == PaneFlexDirection::Row {
			self.row_gap
		}
		else {
			self.column_gap
		}
	}

	//==============================================================================
	/// align-content 를 늘어놓기 꼴로. (stretch 는 줄을 늘인 뒤 처음부터)
	//==============================================================================
	fn align_content_as_justify(&self) -> PaneJustifyContent {
		match self.align_content {
			PaneAlignContent::Stretch | PaneAlignContent::Start => {
				PaneJustifyContent::Start
			}
			PaneAlignContent::Center => {
				PaneJustifyContent::Center
			}
			PaneAlignContent::End => {
				PaneJustifyContent::End
			}
			PaneAlignContent::SpaceBetween => {
				PaneJustifyContent::SpaceBetween
			}
			PaneAlignContent::SpaceAround => {
				PaneJustifyContent::SpaceAround
			}
			PaneAlignContent::SpaceEvenly => {
				PaneJustifyContent::SpaceEvenly
			}
		}
	}

	//==============================================================================
	/// 보이는 칸 크기. (노드 크기에서 테두리와 클래식 스크롤 막대 칸을 던 안쪽 여백 상자)
	//==============================================================================
	fn read_viewport_size(&self, node_size: Vector2) -> Vector2 {
		Vector2::create((node_size.x - self.border_width_list[1] - self.border_width_list[3] - self.scrollbar_gutter.x).max(0.0), (node_size.y - self.border_width_list[0] - self.border_width_list[2] - self.scrollbar_gutter.y).max(0.0))
	}

	//==============================================================================
	/// 폭을 채우는 요소인지. (div 같은 블록 요소, 단추와 입력과 span 은 제 폭)
	//==============================================================================
	fn is_block_level(&self) -> bool {
		!matches!(self.tag_name.as_str(), "button" | "span" | "input" | "label" | "a" | "img" | "canvas")
	}

	//==============================================================================
	/// 스크롤 막대 썸 자리. (노드 좌표, 세로 막대면 is_vertical, 넘치지 않으면 None)
	/// - 얹는 막대는 보이는 칸 가장자리의 두께 8 썸이다. 클래식 막대는 막대 칸 안, 양끝 단추 사이에서 움직이는 크롬 치수의 썸이다. (그린 썸 자리)
	//==============================================================================
	fn calculate_scrollbar_thumb_rect(&self, content_size: Vector2, is_vertical: bool) -> Option<Rect> {
		let viewport_size = self.read_viewport_size(content_size);
		let (overflow, viewport_length, extent_length, offset) = if is_vertical {
			(self.overflow_y, viewport_size.y, self.scroll_extent.y, self.scroll_offset.y)
		}
		else {
			(self.overflow_x, viewport_size.x, self.scroll_extent.x, self.scroll_offset.x)
		};
		if !overflow.is_scrollable() || extent_length <= viewport_length + 0.5 || viewport_length <= 0.0 {
			return None;
		}
		let gutter_thickness = if is_vertical {
			self.scrollbar_gutter.x
		}
		else {
			self.scrollbar_gutter.y
		};
		if gutter_thickness > 0.0 {
			let metrics = crate::web::pane::read_classic_scrollbar_metrics(self.is_thin_scrollbar)?;
			let (thumb_start, thumb_length) = crate::web::pane::calculate_classic_thumb_span(&metrics, viewport_length, viewport_length, extent_length, offset)?;
			let painted_start = thumb_start + metrics.thumb_length_inset;
			let painted_length = (thumb_length - metrics.thumb_length_inset * 2.0).max(0.0);
			return if is_vertical {
				let track_left = content_size.x - self.border_width_list[1] - gutter_thickness;
				Some(Rect::create(track_left + metrics.thumb_cross_offset, self.border_width_list[0] + painted_start, metrics.thumb_thickness, painted_length))
			}
			else {
				let track_top = content_size.y - self.border_width_list[2] - gutter_thickness;
				Some(Rect::create(self.border_width_list[3] + painted_start, track_top + metrics.thumb_cross_offset, painted_length, metrics.thumb_thickness))
			};
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
	/// 썸 끌기의 길이. ((썸 길이, 썸이 움직이는 칸 길이), 끈 거리를 넘길 거리로 바꾼다)
	//==============================================================================
	fn read_thumb_drag_lengths(&self, viewport_length: f64, extent_length: f64, is_vertical: bool) -> (f64, f64) {
		let gutter_thickness = if is_vertical {
			self.scrollbar_gutter.x
		}
		else {
			self.scrollbar_gutter.y
		};
		if gutter_thickness > 0.0 {
			if let Some(metrics) = crate::web::pane::read_classic_scrollbar_metrics(self.is_thin_scrollbar) {
				let thumb_room = (viewport_length - metrics.button_length * 2.0).max(1.0);
				let thumb_length = (thumb_room * viewport_length / extent_length.max(1.0)).round().max(metrics.thumb_minimum_length).min(thumb_room);
				return (thumb_length, thumb_room);
			}
		}
		let thumb_length = (viewport_length * viewport_length / extent_length.max(1.0)).max(SCROLLBAR_MINIMUM_THUMB_LENGTH).min(viewport_length);
		(thumb_length, viewport_length)
	}

	//==============================================================================
	/// 굴려 넘긴 만큼을 내용 끝 안으로. (넘길 수 없는 축은 0, 바뀌었는지)
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
/// 요소 제 크기 재기. (원본 max-content 크기: 글, 자식 블록과 flex 의 합, 안쪽 여백과 테두리. 명시한 px 폭과 높이, min, max 를 따른다)
/// - %는 부모를 알 수 없어 auto 로 본다. absolute 와 display none 자식은 넣지 않는다.
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
	let mut children_width: f64 = 0.0;
	let mut children_height: f64 = 0.0;
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
			element.read_main_gap()
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
/// 줄넘김 가로 flex 의 높이. (테두리 상자 폭에서 CSS Flexbox 9.3 처럼 줄을 모은 높이, 줄넘김 가로 flex 가 아니거나 높이를 정했으면 None)
//==============================================================================
fn measure_wrapped_flex_height(node_tree: &NodeTree, node_id: NodeId, border_box_width: f64) -> Option<f64> {
	let element = node_tree[node_id].get_component::<PaneElement>()?;
	if element.display != PaneDisplay::Flex || element.flex_wrap == PaneFlexWrap::NoWrap || element.flex_direction != PaneFlexDirection::Row || element.height != PaneLength::Auto {
		return None;
	}
	let frame_size = element.read_frame_size();
	let content_main = (border_box_width - frame_size.x).max(0.0);
	let main_gap = element.read_main_gap();
	let cross_gap = element.read_cross_gap();
	let mut line_cross_list: Vec<f64> = Vec::new();
	let mut used_main = 0.0;
	let mut line_cross: f64 = 0.0;
	let mut has_item = false;
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
		let base_size = child_element.flex_basis.resolve(content_main).or_else(|| child_element.width.resolve(content_main)).unwrap_or(child_size.x);
		let main_size = clamp_length(base_size, child_element.minimum_width.resolve(content_main).unwrap_or(0.0), child_element.maximum_width.resolve(content_main).unwrap_or(f64::INFINITY));
		let outer_main = main_size + child_element.margin[1] + child_element.margin[3];
		let outer_cross = child_size.y + child_element.margin[0] + child_element.margin[2];
		if has_item && used_main + main_gap + outer_main > content_main {
			line_cross_list.push(line_cross);
			used_main = outer_main;
			line_cross = outer_cross;
		}
		else {
			used_main += if has_item {
				main_gap + outer_main
			}
			else {
				outer_main
			};
			line_cross = line_cross.max(outer_cross);
		}
		has_item = true;
	}
	if has_item {
		line_cross_list.push(line_cross);
	}
	let lines_height: f64 = line_cross_list.iter().sum::<f64>() + cross_gap * (line_cross_list.len() as f64 - 1.0).max(0.0);
	let mut height = lines_height.max(element.measure_text_size().y) + frame_size.y;
	if let PaneLength::Pixel(maximum_height) = element.maximum_height {
		height = height.min(maximum_height);
	}
	if let PaneLength::Pixel(minimum_height) = element.minimum_height {
		height = height.max(minimum_height);
	}
	Some(height.ceil())
}

//==============================================================================
/// flex 항목 하나. (배치 셈 동안)
//==============================================================================
struct PaneFlexItem {
	node_id: NodeId,
	/// 주축과 교차축의 바깥 여백. (앞, 뒤)
	main_margin: (f64, f64),
	cross_margin: (f64, f64),
	base_size: f64,
	minimum_main: f64,
	maximum_main: f64,
	flex_grow: f64,
	flex_shrink: f64,
	main_size: f64,
	cross_size: f64,
	/// 교차축 min, max 와 줄의 교차 크기로 늘일지. (align-items: stretch 이고 교차 크기가 auto)
	cross_minimum: f64,
	cross_maximum: f64,
	is_cross_stretched: bool,
}

//==============================================================================
/// 남는 자리 나누기. ((처음 자리, 항목 사이에 더할 자리), CSS justify-content 와 align-content 의 늘어놓기)
/// - space-between 은 남는 자리가 없거나 항목이 하나면 처음으로, space-around 와 space-evenly 는 남는 자리가 없으면 가운데로 모은다.
//==============================================================================
fn distribute_space(justify_content: PaneJustifyContent, remaining_space: f64, item_count: f64) -> (f64, f64) {
	match justify_content {
		PaneJustifyContent::Start => {
			(0.0, 0.0)
		}
		PaneJustifyContent::Center => {
			(remaining_space * 0.5, 0.0)
		}
		PaneJustifyContent::End => {
			(remaining_space, 0.0)
		}
		PaneJustifyContent::SpaceBetween => {
			if remaining_space > 0.0 && item_count > 1.0 {
				(0.0, remaining_space / (item_count - 1.0))
			}
			else {
				(0.0, 0.0)
			}
		}
		PaneJustifyContent::SpaceAround => {
			if remaining_space > 0.0 {
				(remaining_space / item_count * 0.5, remaining_space / item_count)
			}
			else {
				(remaining_space * 0.5, 0.0)
			}
		}
		PaneJustifyContent::SpaceEvenly => {
			if remaining_space > 0.0 {
				(remaining_space / (item_count + 1.0), remaining_space / (item_count + 1.0))
			}
			else {
				(remaining_space * 0.5, 0.0)
			}
		}
	}
}

//==============================================================================
/// 말줄임한 글. (글이 폭을 넘으면 글자 묶음 앞부분 + 말줄임표, 넘지 않으면 None)
/// - CSS Overflow 3 의 text-overflow: ellipsis: 말줄임표가 들어갈 만큼 끝의 글자 묶음을 숨긴다. 줄의 첫 글자 묶음은 숨기지 않는다.
//==============================================================================
fn truncate_text_with_ellipsis(text: &str, available_width: f64, measure_width: &dyn Fn(&str) -> f64) -> Option<String> {
	use unicode_segmentation::UnicodeSegmentation;
	if measure_width(text) <= available_width {
		return None;
	}
	let ellipsis_width = measure_width("\u{2026}");
	let mut kept_end = 0;
	for (grapheme_index, grapheme) in text.grapheme_indices(true) {
		let next_end = grapheme_index + grapheme.len();
		if kept_end > 0 && measure_width(&text[..next_end]) + ellipsis_width > available_width {
			break;
		}
		if kept_end == 0 && measure_width(&text[..next_end]) + ellipsis_width > available_width {
			kept_end = next_end;
			break;
		}
		kept_end = next_end;
	}
	Some(format!("{}\u{2026}", &text[..kept_end]))
}

//==============================================================================
/// flex-wrap 값 읽기. (모르는 값은 nowrap)
//==============================================================================
fn parse_flex_wrap(value: &str) -> PaneFlexWrap {
	match value {
		"wrap" => {
			PaneFlexWrap::Wrap
		}
		"wrap-reverse" => {
			PaneFlexWrap::WrapReverse
		}
		_ => {
			PaneFlexWrap::NoWrap
		}
	}
}

//==============================================================================
/// 값을 min, max 안으로. (max 가 min 보다 작으면 min 이 이긴다, CSS 와 같다)
//==============================================================================
//==============================================================================
/// CSS opacity 값 읽기. (수나 백분율, 0 ~ 1 로 자른다. 못 읽으면 None)
//==============================================================================
fn parse_opacity(value: &str) -> Option<f64> {
	let value = value.trim();
	let opacity = match value.strip_suffix('%') {
		Some(percent_text) => {
			percent_text.trim().parse::<f64>().ok()? / 100.0
		}
		None => {
			value.parse::<f64>().ok()?
		}
	};
	Some(opacity.clamp(0.0, 1.0))
}

//==============================================================================
/// 요소의 CSS opacity 를 노드로 옮기기. (크롬처럼 요소와 자손을 묶어 한 번에 얹는다: 노드 투명도와 투명도 층)
//==============================================================================
fn apply_element_opacity(node_tree: &mut NodeTree, node_id: NodeId, opacity: Option<f64>) {
	if let Some(opacity) = opacity {
		if let Some(node) = node_tree.get_node_mut(node_id) {
			node.set_local_opacity(opacity);
			node.set_opacity_layer(true);
		}
	}
}

//==============================================================================
/// 길이를 허용 범위로 자른다.
//==============================================================================
fn clamp_length(value: f64, minimum: f64, maximum: f64) -> f64 {
	value.min(maximum).max(minimum)
}

//==============================================================================
/// "12px" 같은 값 읽기. (단위 없는 수도 받는다)
//==============================================================================
fn parse_pixel(value: &str) -> Option<f64> {
	value.trim().trim_end_matches("px").trim().parse::<f64>().ok()
}

//==============================================================================
/// 크기 값 읽기. (px, %, auto)
//==============================================================================
fn parse_length(value: &str) -> PaneLength {
	let value = value.trim();
	if let Some(percent_text) = value.strip_suffix('%') {
		return percent_text.trim().parse::<f64>().map(PaneLength::Percent).unwrap_or(PaneLength::Auto);
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
/// 여백 줄임 꼴 읽기. (1 ~ 4 값 → 위, 오른쪽, 아래, 왼쪽)
//==============================================================================
fn parse_box_values(value: &str) -> [f64; 4] {
	let value_list: Vec<f64> = value.split_whitespace().map(|part| parse_pixel(part).unwrap_or(0.0)).collect();
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
/// CSS 값 나누기. (빈칸으로 나누되 괄호 안의 빈칸은 두고, "rgba(1, 2, 3, 0.4) transparent" → 둘)
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
/// 테두리 줄임 꼴 읽기. ("1px solid #444" → 굵기와 색, none 이면 0)
//==============================================================================
fn parse_border(value: &str) -> (f64, Option<Color>) {
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
/// 밝기 바꾼 색. (원본 filter: brightness)
//==============================================================================
fn scale_brightness(color: Color, brightness: f64) -> Color {
	Color::create((color.red * brightness).min(1.0), (color.green * brightness).min(1.0), (color.blue * brightness).min(1.0), color.alpha)
}

#[crate::reflect]
impl Component for PaneElement {
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
	/// 컴포넌트 이름. ("PaneElement")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"PaneElement"
	}

	//==============================================================================
	/// 트리에 붙음. (단추와 넘친 자식을 자르는 요소는 interactable)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		if self.is_button || self.is_clipping() {
			node_tree[node_id].set_interactable(true);
		}
	}

	//==============================================================================
	/// 갱신. (단추와 pointer 커서 요소의 올려 둠, absolute 요소의 자리와 크기, 자식 배치)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f64) {
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

	//==============================================================================
	/// 터치 누름. (스크롤 막대 썸을 누르면 썸 끌기 시작)
	//==============================================================================
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

	//==============================================================================
	/// 터치 이동. (썸을 끈 만큼 내용을 넘긴다)
	//==============================================================================
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
		let viewport_size = self.read_viewport_size(content_size);
		if is_vertical {
			let (thumb_length, thumb_room) = self.read_thumb_drag_lengths(viewport_size.y, self.scroll_extent.y, true);
			let track_room = (thumb_room - thumb_length).max(1.0);
			let maximum_offset = (self.scroll_extent.y - viewport_size.y).max(0.0);
			self.scroll_offset.y = start_offset.y + (local_position.y - start_position.y) / track_room * maximum_offset;
		}
		else {
			let (thumb_length, thumb_room) = self.read_thumb_drag_lengths(viewport_size.x, self.scroll_extent.x, false);
			let track_room = (thumb_room - thumb_length).max(1.0);
			let maximum_offset = (self.scroll_extent.x - viewport_size.x).max(0.0);
			self.scroll_offset.x = start_offset.x + (local_position.x - start_position.x) / track_room * maximum_offset;
		}
		self.clamp_scroll_offset(viewport_size);
		self.lay_out_children(context);
	}

	//==============================================================================
	/// 터치 뗌. (썸 끌기가 아니고 단추 안에서 떼면 set_on_click 의 함수를 부른다)
	//==============================================================================
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

	//==============================================================================
	/// 터치 취소. (썸 끌기 끝)
	//==============================================================================
	fn touch_cancel(&mut self, _context: &mut ComponentContext, _view_input_position: Vector2) {
		self.scroll_drag_start = None;
	}

	//==============================================================================
	/// 휠을 받는지. (overflow 가 auto 나 scroll 인 축이 있으면 true)
	//==============================================================================
	fn is_wheel_receiver(&self) -> bool {
		self.overflow_x.is_scrollable() || self.overflow_y.is_scrollable()
	}

	//==============================================================================
	/// 휠. (원본 스크롤 상자처럼 그 축으로 더 넘길 수 있을 때만 받고, 아니면 부모에게 넘긴다)
	//==============================================================================
	fn wheel(&mut self, context: &mut ComponentContext, wheel_delta: Vector2) -> bool {
		if !self.is_wheel_receiver() {
			return false;
		}
		let node_id = context.get_node_id();
		let content_size = context.get_node_tree()[node_id].get_content_size();
		let viewport_size = self.read_viewport_size(content_size);
		let previous_offset = self.scroll_offset;
		self.scroll_offset = Vector2::create(self.scroll_offset.x + wheel_delta.x, self.scroll_offset.y + wheel_delta.y);
		self.clamp_scroll_offset(viewport_size);
		if self.scroll_offset.x == previous_offset.x && self.scroll_offset.y == previous_offset.y {
			return false;
		}
		self.lay_out_children(context);
		true
	}

	//==============================================================================
	/// 자식 출력을 자를지. (overflow 가 visible 이 아닌 축이 있으면 border-radius 의 둥근 크기로 자른다)
	//==============================================================================
	fn get_clip_round_size(&self) -> Option<f64> {
		if self.is_clipping() {
			Some(self.border_radius)
		}
		else {
			None
		}
	}

	//==============================================================================
	/// 출력. (바탕, 테두리, 글. 스크롤 막대는 pop_transform 이 그린다)
	//==============================================================================
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
		// canvas 태그: 넣은 렌더 타겟 화면을 안쪽 상자에 늘여 그린다. (아래가 0 행인 텍스처라 위아래를 뒤집어 짚는다)
		if let Some(canvas_texture) = self.canvas_texture {
			if self.tag_name == "canvas" {
				let frame_left = self.padding[3] + self.border_width_list[3];
				let frame_top = self.padding[0] + self.border_width_list[0];
				let box_size = self.calculate_canvas_box_size(content_size);
				if box_size.x > 0.0 && box_size.y > 0.0 {
					let vertex_count = graphic.write_quad(0, frame_left, frame_top, frame_left + box_size.x, frame_top + box_size.y, 0.0, 1.0, 1.0, 0.0) / FLOATS_PER_VERTEX;
					graphic.draw_vertices(vertex_count, canvas_texture, Color::create(1.0, 1.0, 1.0, 1.0));
				}
			}
		}
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
			// 말줄임: white-space: nowrap 인 글이 overflow 가 visible 이 아닌 상자의 줄 끝을 넘으면 끝 글자 묶음을 숨기고 말줄임표(U+2026)를 붙인다.
			// (CSS Overflow 3 의 text-overflow: ellipsis, 줄의 첫 글자는 줄이지 않고 잘리게 둔다. 넘친 글은 줄 시작부터 놓여 왼쪽 맞춤이 된다)
			let is_truncation_enabled = self.is_text_ellipsis && self.is_nowrap_text && self.overflow_x != PaneOverflow::Visible;
			let truncated_text = if is_truncation_enabled {
				truncate_text_with_ellipsis(&self.text, inner_width, &|measured_text: &str| graphic.measure_text(measured_text).width)
			}
			else {
				None
			};
			let horizontal_align = if truncated_text.is_some() {
				PaneTextAlign::Left
			}
			else {
				horizontal_align
			};
			let drawn_text = truncated_text.as_deref().unwrap_or(&self.text);
			match horizontal_align {
				PaneTextAlign::Left => {
					graphic.set_text_align(TextAlign::Left);
					graphic.draw_fill_text(drawn_text, frame_left, text_y);
				}
				PaneTextAlign::Center => {
					graphic.set_text_align(TextAlign::Center);
					graphic.draw_fill_text(drawn_text, frame_left + inner_width * 0.5, text_y);
				}
				PaneTextAlign::Right => {
					graphic.set_text_align(TextAlign::Right);
					graphic.draw_fill_text(drawn_text, frame_left + inner_width, text_y);
				}
			}
		}
	}

	//==============================================================================
	/// 자식을 그린 뒤 스크롤 막대. (자르기가 끝난 뒤라 막대가 내용 위에 얹힌다)
	//==============================================================================
	fn pop_transform(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.display == PaneDisplay::None || !self.is_clipping() {
			return;
		}
		let content_size = transform.get_content_size();
		// 클래식 막대는 막대 칸에 길(scrollbar-color 의 둘째 값)과 양끝 단추를 깐다. 두 막대가 다 있으면 모서리 칸도. (단추 화살표는 그리지 않는다)
		if self.scrollbar_gutter.x > 0.0 || self.scrollbar_gutter.y > 0.0 {
			if let Some(track_color) = self.scrollbar_track_color {
				graphic.set_fill_color(track_color);
				let track_right = content_size.x - self.border_width_list[1];
				let track_bottom = content_size.y - self.border_width_list[2];
				if self.scrollbar_gutter.x > 0.0 {
					graphic.draw_rect(Rect::create(track_right - self.scrollbar_gutter.x, self.border_width_list[0], self.scrollbar_gutter.x, (track_bottom - self.border_width_list[0]).max(0.0)));
				}
				if self.scrollbar_gutter.y > 0.0 {
					graphic.draw_rect(Rect::create(self.border_width_list[3], track_bottom - self.scrollbar_gutter.y, (track_right - self.scrollbar_gutter.x - self.border_width_list[3]).max(0.0), self.scrollbar_gutter.y));
				}
			}
		}
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
	/// absolute 요소의 자리와 크기. (부모 크기에 대고, left 와 right 가 다 있고 폭이 auto 면 그 사이를 채운다)
	//==============================================================================
	fn place_absolute(&self, parent_size: Vector2, intrinsic_size: Vector2) -> (Vector2, Vector2) {
		let resolve_axis = |start: Option<f64>, end: Option<f64>, length: PaneLength, minimum: PaneLength, maximum: PaneLength, parent_length: f64, intrinsic_length: f64| -> (f64, f64) {
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
			size = clamp_length(size, minimum.resolve(parent_length).unwrap_or(0.0), maximum.resolve(parent_length).unwrap_or(f64::INFINITY));
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
	/// 자식 배치. (absolute 는 부모 크기에, 그 밖은 안쪽 여백 안에서 블록(세로로 차례) 또는 flex 로. 넘침을 굴려 넘긴 만큼 옮긴다)
	//==============================================================================
	fn lay_out_children(&mut self, context: &mut ComponentContext) {
		// 클래식 스크롤 막대는 테두리 안, 안쪽 여백 밖의 자리를 차지한다. scroll 은 늘, auto 는 넘칠 때만. (CSS Overflow 3 의 스크롤 막대 칸)
		// auto 는 막대 없이 놓아 보고 넘치면 막대 자리를 덜고 다시 놓는다. (크롬 LayoutBox 의 스크롤 막대를 넣은 다시 배치)
		let scrollbar_thickness = if self.is_clipping() {
			crate::web::pane::read_classic_scrollbar_metrics(self.is_thin_scrollbar).map(|metrics| metrics.thickness).unwrap_or(0.0)
		}
		else {
			0.0
		};
		let read_gutter = |overflow: PaneOverflow, is_overflowing: bool| -> f64 {
			if overflow == PaneOverflow::Scroll || (overflow == PaneOverflow::Auto && is_overflowing) {
				scrollbar_thickness
			}
			else {
				0.0
			}
		};
		self.scrollbar_gutter = Vector2::create(read_gutter(self.overflow_y, false), read_gutter(self.overflow_x, false));
		self.place_children(context);
		if scrollbar_thickness > 0.0 {
			for _ in 0..2 {
				let node_size = context.get_node_tree()[context.get_node_id()].get_content_size();
				let viewport_size = self.read_viewport_size(node_size);
				let next_gutter = Vector2::create(read_gutter(self.overflow_y, self.scroll_extent.y > viewport_size.y + 0.5), read_gutter(self.overflow_x, self.scroll_extent.x > viewport_size.x + 0.5));
				let gutter = Vector2::create(next_gutter.x.max(self.scrollbar_gutter.x), next_gutter.y.max(self.scrollbar_gutter.y));
				if gutter.x == self.scrollbar_gutter.x && gutter.y == self.scrollbar_gutter.y {
					break;
				}
				self.scrollbar_gutter = gutter;
				self.place_children(context);
			}
		}
	}

	//==============================================================================
	/// 자식 놓기. (스크롤 막대 칸을 던 안쪽 여백 안에서 블록 또는 flex, 넘침을 굴려 넘긴 만큼 옮긴다)
	//==============================================================================
	fn place_children(&mut self, context: &mut ComponentContext) {
		let node_id = context.get_node_id();
		let node_size = context.get_node_tree()[node_id].get_content_size();
		let frame_size = self.read_frame_size();
		let content_left = self.padding[3] + self.border_width_list[3];
		let content_top = self.padding[0] + self.border_width_list[0];
		let content_width = (node_size.x - frame_size.x - self.scrollbar_gutter.x).max(0.0);
		let content_height = (node_size.y - frame_size.y - self.scrollbar_gutter.y).max(0.0);
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
					main_intrinsic.min(main_length.resolve(content_main).unwrap_or(f64::INFINITY))
				}
				else {
					0.0
				};
				let minimum_main = main_minimum.resolve(content_main).unwrap_or(automatic_minimum);
				let maximum_main = main_maximum.resolve(content_main).unwrap_or(f64::INFINITY);
				// 교차축 크기: 정한 값, 아니면 가정 크기(제 크기). stretch 는 줄의 교차 크기를 안 뒤에 늘린다. (CSS Flexbox 9.4)
				let cross_minimum = cross_minimum.resolve(content_cross).unwrap_or(0.0);
				let cross_maximum = cross_maximum.resolve(content_cross).unwrap_or(f64::INFINITY);
				let is_cross_stretched = cross_length.resolve(content_cross).is_none() && self.align_items == PaneAlignItems::Stretch;
				let cross_size = clamp_length(cross_length.resolve(content_cross).unwrap_or(cross_intrinsic), cross_minimum, cross_maximum);
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
					cross_minimum,
					cross_maximum,
					is_cross_stretched,
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
				let width = clamp_length(child_element.width.resolve(content_width).unwrap_or(auto_width), child_element.minimum_width.resolve(content_width).unwrap_or(0.0), child_element.maximum_width.resolve(content_width).unwrap_or(f64::INFINITY));
				// 높이가 auto 인 줄넘김 가로 flex 는 이 폭에서 모은 줄의 높이다. (제 크기는 한 줄로 잰 값이라)
				let auto_height = measure_wrapped_flex_height(context.get_node_tree(), child_id, width).unwrap_or(intrinsic_size.y);
				let height = clamp_length(child_element.height.resolve(content_height).unwrap_or(auto_height), child_element.minimum_height.resolve(content_height).unwrap_or(0.0), child_element.maximum_height.resolve(content_height).unwrap_or(f64::INFINITY));
				flow_y += margin[0];
				placement_list.push((child_id, Some((Vector2::create(content_left + margin[3], flow_y), Vector2::create(width, height))), true));
				flow_y += height + margin[2];
			}
		}
		if !flex_item_list.is_empty() {
			let main_gap = self.read_main_gap();
			let cross_gap = self.read_cross_gap();
			// 1. 줄 모으기. (CSS Flexbox 9.3: 줄넘김이 있으면 바깥 가정 주축 크기를 차례로 더하다 처음 넘치는 항목에서 새 줄, 첫 항목은 넘쳐도 그 줄에)
			let mut line_list: Vec<Vec<PaneFlexItem>> = Vec::new();
			let mut used_main = 0.0;
			for item in flex_item_list {
				let outer_main = item.main_size + item.main_margin.0 + item.main_margin.1;
				let is_new_line = match line_list.last() {
					Some(line) => {
						self.flex_wrap != PaneFlexWrap::NoWrap && !line.is_empty() && used_main + main_gap + outer_main > content_main
					}
					None => {
						true
					}
				};
				if is_new_line {
					line_list.push(Vec::new());
					used_main = outer_main;
				}
				else {
					used_main += main_gap + outer_main;
				}
				if let Some(line) = line_list.last_mut() {
					line.push(item);
				}
			}
			// 2. 줄마다 늘고 줄 길이 정하기. (9.7)
			for line in line_list.iter_mut() {
				self.resolve_flexible_lengths(line, content_main);
			}
			// 3. 줄의 교차 크기. (9.4: 한 줄 상자는 상자의 안쪽 교차 크기, 여러 줄은 항목 바깥 교차 크기의 가장 큰 값, align-content: stretch 면 남는 자리를 줄마다 고루 더한다)
			let is_single_line = self.flex_wrap == PaneFlexWrap::NoWrap;
			let mut line_cross_list: Vec<f64> = line_list
				.iter()
				.map(|line| {
					if is_single_line {
						content_cross
					}
					else {
						line.iter().map(|item| item.cross_size + item.cross_margin.0 + item.cross_margin.1).fold(0.0, f64::max)
					}
				})
				.collect();
			let line_count = line_list.len() as f64;
			let used_cross: f64 = line_cross_list.iter().sum::<f64>() + cross_gap * (line_count - 1.0);
			let mut remaining_cross = content_cross - used_cross;
			if !is_single_line && self.align_content == PaneAlignContent::Stretch && remaining_cross > 0.0 {
				for line_cross in line_cross_list.iter_mut() {
					*line_cross += remaining_cross / line_count;
				}
				remaining_cross = 0.0;
			}
			// 4. 줄 늘어놓기. (8.4 align-content, 한 줄 상자는 줄이 상자를 채운다)
			let (mut line_position, line_between_space) = if is_single_line {
				(0.0, 0.0)
			}
			else {
				distribute_space(self.align_content_as_justify(), remaining_cross, line_count)
			};
			for (line_index, line) in line_list.iter_mut().enumerate() {
				let line_cross = line_cross_list[line_index];
				// 줄 안 주축 늘어놓기. (justify-content)
				let line_used_main: f64 = line.iter().map(|item| item.main_size + item.main_margin.0 + item.main_margin.1).sum::<f64>() + main_gap * (line.len() as f64 - 1.0);
				let (mut main_position, between_space) = distribute_space(self.justify_content, content_main - line_used_main, line.len() as f64);
				for item in line.iter_mut() {
					// stretch 항목은 줄의 교차 크기를 채운다. (9.4 의 11)
					if item.is_cross_stretched {
						item.cross_size = clamp_length((line_cross - item.cross_margin.0 - item.cross_margin.1).max(0.0), item.cross_minimum, item.cross_maximum);
					}
					main_position += item.main_margin.0;
					let cross_in_line = match self.align_items {
						PaneAlignItems::Center => {
							(line_cross - item.cross_size - item.cross_margin.0 - item.cross_margin.1) * 0.5 + item.cross_margin.0
						}
						PaneAlignItems::End => {
							line_cross - item.cross_size - item.cross_margin.1
						}
						PaneAlignItems::Start | PaneAlignItems::Stretch => {
							item.cross_margin.0
						}
					};
					// wrap-reverse 는 교차축의 시작과 끝을 바꾼다. (줄이 끝에서 쌓이고 flex-start 가 줄의 끝 쪽)
					let mut cross_position = line_position + cross_in_line;
					if self.flex_wrap == PaneFlexWrap::WrapReverse {
						cross_position = content_cross - cross_position - item.cross_size;
					}
					let (position, size) = if is_row {
						(Vector2::create(content_left + main_position, content_top + cross_position), Vector2::create(item.main_size, item.cross_size))
					}
					else {
						(Vector2::create(content_left + cross_position, content_top + main_position), Vector2::create(item.cross_size, item.main_size))
					};
					placement_list.push((item.node_id, Some((position, size)), true));
					main_position += item.main_size + item.main_margin.1 + main_gap + between_space;
				}
				line_position += line_cross + cross_gap + line_between_space;
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
			let viewport_size = self.read_viewport_size(node_size);
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
	/// flex 길이 정하기. (CSS Flexbox 9.7: 남는 자리는 grow 비로 나누고, 모자라면 shrink 와 기준 크기를 곱한 비로 줄인다. min, max 에 걸린 항목은 묶고 다시 나눈다)
	//==============================================================================
	fn resolve_flexible_lengths(&self, item_list: &mut [PaneFlexItem], content_main: f64) {
		let gap_total = self.read_main_gap() * (item_list.len() as f64 - 1.0);
		let read_outer = |item: &PaneFlexItem, size: f64| -> f64 {
			size + item.main_margin.0 + item.main_margin.1
		};
		// 1. 키울지 줄일지. (가정 크기 합과 견준다)
		let hypothetical_total: f64 = item_list.iter().map(|item| read_outer(item, item.main_size)).sum::<f64>() + gap_total;
		let is_growing = hypothetical_total < content_main;
		let read_factor = |item: &PaneFlexItem| -> f64 {
			if is_growing {
				item.flex_grow
			}
			else {
				item.flex_shrink
			}
		};
		// 2. 늘지 않는 항목은 가정 크기로 묶는다.
		let mut is_frozen_list: Vec<bool> = item_list.iter().map(|item| read_factor(item) == 0.0 || (is_growing && item.base_size > item.main_size) || (!is_growing && item.base_size < item.main_size)).collect();
		let read_remaining_space = |item_list: &[PaneFlexItem], is_frozen_list: &[bool]| -> f64 {
			let used_total: f64 = item_list
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
				.sum::<f64>() + gap_total;
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
			let factor_total: f64 = item_list.iter().zip(is_frozen_list.iter()).filter(|(_, is_frozen)| !**is_frozen).map(|(item, _)| read_factor(item)).sum();
			if factor_total < 1.0 {
				let limited_space = initial_free_space * factor_total;
				if limited_space.abs() < remaining_space.abs() {
					remaining_space = limited_space;
				}
			}
			let scaled_shrink_total: f64 = item_list.iter().zip(is_frozen_list.iter()).filter(|(_, is_frozen)| !**is_frozen).map(|(item, _)| item.flex_shrink * item.base_size).sum();
			let mut violation_list: Vec<f64> = vec![0.0; item_list.len()];
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
	/// 테두리. (네 쪽이 같으면 둥근 사각형 선, 아니면 쪽마다 띠)
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
/// 화면 자리 → 노드 좌표. (거꿀 행렬이 없으면 None)
//==============================================================================
fn convert_view_to_local(node_tree: &NodeTree, node_id: NodeId, view_position: Vector2) -> Option<Vector2> {
	let world_matrix = node_tree.calculate_world_matrix(node_id);
	let inverse_matrix = world_matrix.invert()?;
	let local_point = inverse_matrix.transform_point(view_position.x, view_position.y);
	Some(Vector2::create(local_point.x, local_point.y))
}

//==============================================================================
/// 스타일 및 요소 생성 유틸리티. (원본 PaneStyle)
///
/// # 예제
/// 도구 막대에 단추를 놓고 스타일을 나중에 바꾸기
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, NodeTree, PaneStyle, PaneStyleOptions, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// node_tree[root_id].set_content_size(Vector2::create(600.0, 40.0));
/// let toolbar_id = PaneStyle::create(&mut node_tree, "div", "toolbar", &PaneStyleOptions::default());
/// let play_button_id = PaneStyle::create(&mut node_tree, "button", "button", &PaneStyleOptions { text: "Play", accent: true, ..Default::default() });
/// node_tree.add_child(root_id, toolbar_id);
/// node_tree.add_child(toolbar_id, play_button_id);
/// PaneStyle::apply(&mut node_tree, toolbar_id, &[("justify-content", "flex-start"), ("padding", "0 8px")]);
/// node_tree.tick(&engine, 1.0 / 60.0);
/// node_tree.tick(&engine, 1.0 / 60.0);
/// assert_eq!(node_tree[toolbar_id].get_content_size(), Vector2::create(600.0, 40.0));
/// assert_eq!(node_tree[play_button_id].get_local_position().x, 8.0);
/// ```
//==============================================================================
pub struct PaneStyle;

impl PaneStyle {
	//==============================================================================
	/// 요소에 여러 스타일을 한꺼번에 넣는다. ((CSS 속성 이름, 값) 목록, PaneElement 가 없는 노드면 아무것도 하지 않는다)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `node_id`: 스타일을 넣을 요소 노드.
	/// - `styles`: (CSS 속성 이름, 값) 목록.
	//==============================================================================
	pub fn apply(node_tree: &mut NodeTree, node_id: NodeId, styles: &[(&str, &str)]) {
		let mut opacity = None;
		if let Some(element) = node_tree.get_node_mut(node_id).and_then(|node| node.get_component_mut::<PaneElement>()) {
			for (property_name, property_value) in styles {
				element.apply_style(property_name, property_value);
			}
			opacity = element.opacity;
		}
		apply_element_opacity(node_tree, node_id, opacity);
	}

	//==============================================================================
	/// 스타일 프리셋을 넣은 요소 노드를 만든다. (떨어진 노드, node_tree.add_child 로 붙인다)
	/// (preset: "panel" | "toolbar" | "button" | "input" | "textarea" | "text", 그 밖은 공통 스타일만)
	///
	/// # 인자
	/// - `node_tree`: 노드를 만들 트리.
	/// - `tag_name`: 태그 이름. (input 과 textarea 는 엔진 입력 필드가 글을 받는다)
	/// - `preset`: 스타일 프리셋. ("panel" | "toolbar" | "button" | "input" | "textarea" | "text")
	/// - `options`: id, 글, 덧붙이는 스타일, 안내 글, 단추 색.
	///
	/// # 돌려줌
	/// 만든 요소 노드. (떨어진 노드)
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
		apply_element_opacity(node_tree, node_id, element.opacity);
		let is_input = element.tag_name == "input";
		let is_text_area = element.tag_name == "textarea";
		let font_size = element.font_size;
		let font_family = element.font_family.clone();
		let text_color = element.color;
		let padding = element.padding[3];
		let line_height = element.line_height;
		// textarea 의 글은 입력 필드가 든다. (원본 innerText 가 textarea 의 처음 값이 된다)
		let initial_text = if is_text_area {
			std::mem::take(&mut element.text)
		}
		else {
			String::new()
		};
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
		// textarea 는 엔진 여러 줄 입력 필드가 글을 받는다. (원본 DOM textarea: 프리셋은 테두리와 포커스 테두리가 없고 탭 크기 4, 커서는 글 색)
		if is_text_area {
			if let Some(field) = node_tree.add_component(node_id, UIInputField::new()) {
				field.set_multiline(true);
				field.set_background_color(Color::create(0.0, 0.0, 0.0, 0.0));
				field.set_text_color(text_color);
				field.set_cursor_color(text_color);
				field.set_font_size(font_size);
				field.set_font_family(&font_family);
				field.set_padding(padding);
				field.set_round_size(0.0);
				field.set_line_height(line_height);
				field.set_tab_size(4.0);
				field.set_placeholder(options.placeholder);
				field.set_placeholder_color(PaneTheme::COLOR_TEXT_DIM);
				field.set_focus_border_width(0.0);
				field.set_text(&initial_text);
				field.set_use_dom_input(true);
				field.attach();
			}
		}
		node_id
	}
}

//==============================================================================
/// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::TouchKind;

	//==============================================================================
	/// 노드로 지은 패널: 붙이면 부모 크기로 놓이고, 숨기면 리사이저와 함께 빠지고, 끌면 앞 자식 크기가 바뀐다. 부모 크기가 바뀌면 다시 놓는다.
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
	/// PaneStyle 프리셋: 도구 막대는 단추를 가로 가운데에 모으고, 패널은 안쪽 여백 안에 차례로, 단추를 누르고 떼면 함수를 부른다.
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
	/// 원본 에디터의 꼴: 세로 flex 패널(제목 + flex 1 의 스크롤 목록), 가로 flex 머리줄(flex 1 의 빈칸이 오른쪽 아이콘을 민다). 휠과 끌기로 목록을 넘긴다.
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
	/// 줄넘김 flex: 헤드리스 크롬이 놓은 자리와 같다. (CSS Flexbox 9.3 줄 모으기, 9.4 align-content: stretch, wrap-reverse, 세로 줄넘김, 블록 안 auto 높이)
	//==============================================================================
	#[test]
	fn pane_flex_wrap_matches_chrome() {
		let engine = Engine::new(EngineConfiguration::new());
		let lay_out = |container_style: &[(&str, &str)], item_style_list: &[&[(&str, &str)]]| -> Vec<[f64; 4]> {
			let mut node_tree = NodeTree::new();
			let root_id = node_tree.get_root_id();
			node_tree[root_id].set_content_size(Vector2::create(800.0, 600.0));
			let holder_id = node_tree.create_child(root_id, "holder");
			{
				let holder_node = &mut node_tree[holder_id];
				holder_node.set_pivot(Pivot::TOP_LEFT);
				holder_node.set_anchor(Vector2::zero());
				holder_node.set_content_size(Vector2::create(800.0, 600.0));
			}
			let container_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: container_style, ..Default::default() });
			node_tree.add_child(holder_id, container_id);
			let item_id_list: Vec<NodeId> = item_style_list
				.iter()
				.map(|item_style| {
					let item_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: item_style, ..Default::default() });
					node_tree.add_child(container_id, item_id);
					item_id
				})
				.collect();
			for _ in 0..3 {
				node_tree.tick(&engine, 1.0 / 60.0);
			}
			item_id_list
				.iter()
				.map(|item_id| {
					let position = node_tree[*item_id].get_local_position();
					let size = node_tree[*item_id].get_content_size();
					[position.x, position.y, size.x, size.y]
				})
				.collect()
		};
		let assert_rects = |actual_list: Vec<[f64; 4]>, expected_list: &[[f64; 4]]| {
			for (actual, expected) in actual_list.iter().zip(expected_list.iter()) {
				for (actual_value, expected_value) in actual.iter().zip(expected.iter()) {
					assert!((actual_value - expected_value).abs() < 0.01, "{:?} != {:?}", actual_list, expected_list);
				}
			}
		};
		let item_list: [&[(&str, &str)]; 5] = [
			&[("position", "relative"), ("width", "120px"), ("height", "30px")],
			&[("position", "relative"), ("width", "100px"), ("height", "40px")],
			&[("position", "relative"), ("width", "90px"), ("height", "20px")],
			&[("position", "relative"), ("width", "150px"), ("height", "50px")],
			&[("position", "relative"), ("width", "60px"), ("height", "25px")],
		];
		let container_base: [(&str, &str); 5] = [("display", "flex"), ("width", "300px"), ("height", "200px"), ("flex-wrap", "wrap"), ("gap", "10px 20px")];
		// align-content 기본(stretch): 남는 65 를 세 줄에 고루. (크롬 71.671875, 153.328125)
		assert_rects(lay_out(&container_base, &item_list), &[[0.0, 0.0, 120.0, 30.0], [140.0, 0.0, 100.0, 40.0], [0.0, 71.666664, 90.0, 20.0], [110.0, 71.666664, 150.0, 50.0], [0.0, 153.33333, 60.0, 25.0]]);
		let mut center_style = container_base.to_vec();
		center_style.push(("align-content", "center"));
		assert_rects(lay_out(&center_style, &item_list), &[[0.0, 32.5, 120.0, 30.0], [140.0, 32.5, 100.0, 40.0], [0.0, 82.5, 90.0, 20.0], [110.0, 82.5, 150.0, 50.0], [0.0, 142.5, 60.0, 25.0]]);
		// 높이 auto 항목은 늘인 줄의 교차 크기를 채운다.
		let auto_item_list: [&[(&str, &str)]; 5] = [item_list[0], &[("position", "relative"), ("width", "100px"), ("height", "auto")], item_list[2], item_list[3], &[("position", "relative"), ("width", "60px"), ("height", "auto")]];
		assert_rects(lay_out(&container_base, &auto_item_list), &[[0.0, 0.0, 120.0, 30.0], [140.0, 0.0, 100.0, 63.333332], [0.0, 73.33333, 90.0, 20.0], [110.0, 73.33333, 150.0, 50.0], [0.0, 166.66667, 60.0, 33.333332]]);
		// wrap-reverse: 줄이 아래에서 쌓이고 flex-start 는 줄의 아래.
		let reverse_style: [(&str, &str); 6] = [("display", "flex"), ("width", "300px"), ("height", "200px"), ("flex-wrap", "wrap-reverse"), ("gap", "10px 20px"), ("align-items", "flex-start")];
		assert_rects(lay_out(&reverse_style, &item_list), &[[0.0, 170.0, 120.0, 30.0], [140.0, 160.0, 100.0, 40.0], [0.0, 108.33333, 90.0, 20.0], [110.0, 78.33333, 150.0, 50.0], [0.0, 21.666668, 60.0, 25.0]]);
		// 세로 줄넘김.
		let column_style: [(&str, &str); 6] = [("display", "flex"), ("flex-direction", "column"), ("width", "300px"), ("height", "100px"), ("flex-wrap", "wrap"), ("gap", "10px")];
		let column_item_list: [&[(&str, &str)]; 4] = [
			&[("position", "relative"), ("width", "50px"), ("height", "40px")],
			&[("position", "relative"), ("width", "70px"), ("height", "50px")],
			&[("position", "relative"), ("width", "40px"), ("height", "30px")],
			&[("position", "relative"), ("width", "60px"), ("height", "60px")],
		];
		assert_rects(lay_out(&column_style, &column_item_list), &[[0.0, 0.0, 50.0, 40.0], [0.0, 50.0, 70.0, 50.0], [160.0, 0.0, 40.0, 30.0], [160.0, 40.0, 60.0, 60.0]]);
		// align-content: space-between 과 justify-content: center.
		let between_style: [(&str, &str); 6] = [("display", "flex"), ("width", "300px"), ("height", "200px"), ("flex-wrap", "wrap"), ("align-content", "space-between"), ("justify-content", "center")];
		assert_rects(lay_out(&between_style, &item_list), &[[40.0, 0.0, 120.0, 30.0], [160.0, 0.0, 100.0, 40.0], [0.0, 150.0, 90.0, 20.0], [90.0, 150.0, 150.0, 50.0], [240.0, 150.0, 60.0, 25.0]]);
		// 블록 안의 높이 auto 줄넘김 flex 는 이 폭에서 모은 줄 높이. (크롬 70)
		let block_item_list: [&[(&str, &str)]; 1] = [&[("position", "relative"), ("display", "flex"), ("flex-wrap", "wrap"), ("gap", "10px"), ("height", "auto")]];
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(300.0, 600.0));
		let block_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("width", "300px"), ("height", "600px")], ..Default::default() });
		node_tree.add_child(root_id, block_id);
		let wrap_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: block_item_list[0], ..Default::default() });
		node_tree.add_child(block_id, wrap_id);
		for _ in 0..3 {
			let item_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("position", "relative"), ("width", "120px"), ("height", "30px")], ..Default::default() });
			node_tree.add_child(wrap_id, item_id);
		}
		for _ in 0..3 {
			node_tree.tick(&engine, 1.0 / 60.0);
		}
		assert!((node_tree[wrap_id].get_content_size().y - 70.0).abs() < 0.01);
	}

	//==============================================================================
	/// 말줄임: 폭을 넘는 글은 끝 글자 묶음을 숨기고 말줄임표를 붙인다. 첫 글자는 숨기지 않는다.
	//==============================================================================
	#[test]
	fn pane_text_ellipsis_truncates_like_css() {
		let measure_fixed = |text: &str| -> f64 {
			text.chars().count() as f64 * 10.0
		};
		assert_eq!(truncate_text_with_ellipsis("Hello", 50.0, &measure_fixed), None);
		assert_eq!(truncate_text_with_ellipsis("Hello world", 50.0, &measure_fixed).as_deref(), Some("Hell\u{2026}"));
		assert_eq!(truncate_text_with_ellipsis("Hello", 5.0, &measure_fixed).as_deref(), Some("H\u{2026}"));
		assert_eq!(truncate_text_with_ellipsis("e\u{301}xyz", 25.0, &measure_fixed).as_deref(), Some("e\u{301}\u{2026}"));
	}

	//==============================================================================
	/// 스크롤 막대 칸: 클래식 막대(윈도우, 리눅스, 맥 클래식 설정)는 넘칠 때 내용 폭에서 막대 두께를 던다. 얹는 막대는 덜지 않는다.
	//==============================================================================
	#[test]
	fn pane_classic_scrollbar_takes_space() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		node_tree[root_id].set_content_size(Vector2::create(200.0, 100.0));
		let scroller_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("width", "200px"), ("height", "100px"), ("overflow", "auto")], ..Default::default() });
		node_tree.add_child(root_id, scroller_id);
		let child_id = PaneStyle::create(&mut node_tree, "div", "", &PaneStyleOptions { style: &[("position", "relative"), ("height", "300px")], ..Default::default() });
		node_tree.add_child(scroller_id, child_id);
		for _ in 0..3 {
			node_tree.tick(&engine, 1.0 / 60.0);
		}
		let thickness = crate::web::pane::read_classic_scrollbar_metrics(false).map(|metrics| metrics.thickness).unwrap_or(0.0);
		assert!((node_tree[child_id].get_content_size().x - (200.0 - thickness)).abs() < 0.01);
		// 넘치지 않으면 auto 는 막대가 없다.
		PaneStyle::apply(&mut node_tree, child_id, &[("height", "50px")]);
		node_tree.tick(&engine, 1.0 / 60.0);
		assert!((node_tree[child_id].get_content_size().x - 200.0).abs() < 0.01);
		// scroll 은 넘치지 않아도 막대 자리를 둔다.
		PaneStyle::apply(&mut node_tree, scroller_id, &[("overflow", "scroll")]);
		node_tree.tick(&engine, 1.0 / 60.0);
		assert!((node_tree[child_id].get_content_size().x - (200.0 - thickness)).abs() < 0.01);
		// 크롬 치수: 윈도우, 리눅스 Fluent 15 (thin 10), 맥 클래식 15 (thin 11).
		if let Some(metrics) = crate::web::pane::read_classic_scrollbar_metrics(false) {
			assert_eq!(metrics.thickness, 15.0);
		}
		if let Some(metrics) = crate::web::pane::read_classic_scrollbar_metrics(true) {
			assert!(metrics.thickness == 10.0 || metrics.thickness == 11.0);
		}
	}

	//==============================================================================
	/// flex 길이 셈: grow 비, shrink 와 기준 크기의 비, max 에 걸린 항목은 묶고 나머지에 다시 나눈다. justify-content space-between.
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
