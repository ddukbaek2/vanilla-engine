//==============================================================================
//! 재활용 리스트 뷰. (원본 ui/uilistview.js 의 UIListView)
//! - 항목 수만 받아, 화면에 보이는 범위(앞뒤 여유 칸 포함)만 실제 노드로 만들고 벗어난 노드는 회수해 다시 쓴다.
//!   수백 ~ 수천 건이어도 노드 수는 화면 분량에 머문다. 항목 내용은 set_bind_item_event 로 코드에서 채운다. (데이터 주도)
//! - 끝 근처에 닿으면 set_reach_end_event 알림을 한 번 보낸다. (무한 스크롤 / 더 불러오기) set_item_count 로 항목 수가 늘면 다시 무장된다.
//! - 원본처럼 스크롤 뷰와 같은 노드에 붙는다. 노드에 스크롤 뷰가 없으면 붙을 때 UIScrollView 를 함께 붙인다. (원본 require)
//!   UISnapScrollView 가 붙은 노드면 그 품은 뷰를 쓴다. (원본 getComponents(UIScrollView) 가 하위 클래스까지 잡는 자리)
//!   스크롤 뷰가 먼저 붙어 먼저 tick 하므로, 이 위젯은 그 프레임의 스크롤 오프셋으로 보이는 범위를 고른다.
//! - 사용:
//!   ```text
//!     let list_view = node_tree.add_component(list_id, UIListView::new()).expect("list view");
//!     list_view.set_create_item_event(Box::new(|tree: &mut NodeTree| { let item_id = tree.create_node("item"); ...; item_id }));
//!     list_view.set_bind_item_event(Box::new(|tree: &mut NodeTree, item_id: NodeId, item_index: usize| { ... }));
//!     list_view.set_item_count(500);
//!   ```
//! - 원본과 다른 것:
//!   - set_template_node 는 원본처럼 템플릿 노드를 UIDocument 로 직렬화해 두고 항목마다 복제한다. 원본은 반사로 모든 컴포넌트를 적지만,
//!     러스트 판은 UIDocument 에 등록한 종류만 복제한다. (게임 컴포넌트는 set_template_node_with_document 에 등록을 마친 UIDocument 를 넘긴다)
//!   - 항목 노드를 만드는 함수(set_create_item_event)도 받는다. (러스트 판에서 먼저 생긴 범용 기능, 원본에도 넣었다) 만든 노드는 content 노드의 자식으로 옮겨 붙인다.
//!     항목 크기를 정하지 않았으면(0) 원본이 템플릿 크기를 읽듯 처음 만든 항목 노드의 진행 축 content_size 를 읽는다.
//!   - 바인드 이벤트는 (트리, 항목 노드, 항목 번호) 를 받는다. 끝 알림은 리스트 뷰와 문맥을 받는다.
//!   - set_item_count, set_item_size, set_spacing, refresh 는 값을 정해 두고 다음 tick 에 스크롤 영역 갱신과 다시 바인딩을 한다.
//!     (원본은 그 자리에서 한다. 같은 노드의 스크롤 뷰는 씬 코드가 이 위젯을 쥐고 있는 동안 닿을 수 없다)
//!   - 활성 항목은 원본 Map 처럼 넣은 순서로 든다.
//==============================================================================
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uidocument::UIDocument;
use crate::ui::uiscrollview::UIScrollView;
use crate::ui::uisnapscrollview::{find_scroll_view, find_scroll_view_mut, UISnapScrollView};

//==============================================================================
/// 항목 노드 만들기 함수. (트리를 받아 새 항목 노드를 돌려준다. 원본 템플릿 복제 자리)
//==============================================================================
pub type UIListViewCreateItemEvent = Box<dyn FnMut(&mut NodeTree) -> NodeId>;

//==============================================================================
/// 바인드 이벤트. (트리, 항목 노드, 항목 번호)
//==============================================================================
pub type UIListViewBindItemEvent = Box<dyn FnMut(&mut NodeTree, NodeId, usize)>;

//==============================================================================
/// 끝 알림. (리스트 뷰와 문맥을 받는다)
//==============================================================================
pub type UIListViewReachEndEvent = Box<dyn FnMut(&mut UIListView, &mut ComponentContext)>;

//==============================================================================
/// 재활용 리스트 뷰.
/// - 같은 노드의 스크롤 뷰(없으면 붙을 때 UIScrollView 를 함께 붙인다) 오프셋으로 보이는 항목만 content 노드 아래에 노드로 둔다.
/// - 항목 노드를 만드는 함수(set_create_item_event 나 set_template_node)가 없으면 tick 에서 아무것도 하지 않는다.
///
/// # 예제
/// 항목 500 개 가운데 보이는 다섯 개만 노드로 만들기
/// ```rust
/// use std::cell::RefCell;
/// use std::rc::Rc;
/// use vanilla::{Engine, EngineConfiguration, NodeId, NodeTree, Pivot, UIListView, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let list_id = node_tree.create_child(root_id, "list");
/// node_tree[list_id].set_pivot(Pivot::TOP_LEFT);
/// node_tree[list_id].set_content_size(Vector2::create(200.0, 100.0));
/// let bound_index_list = Rc::new(RefCell::new(Vec::new()));
/// let bound_index_recorder = bound_index_list.clone();
///
/// let list_view = node_tree.add_component(list_id, UIListView::new()).expect("list view");
/// list_view.set_create_item_event(Box::new(|tree: &mut NodeTree| {
/// 	let item_id = tree.create_node("item");
/// 	tree[item_id].set_pivot(Pivot::TOP_LEFT);
/// 	tree[item_id].set_content_size(Vector2::create(200.0, 30.0));
/// 	item_id
/// }));
/// list_view.set_bind_item_event(Box::new(move |_tree: &mut NodeTree, _item_id: NodeId, item_index: usize| {
/// 	bound_index_recorder.borrow_mut().push(item_index);
/// }));
/// list_view.set_item_count(500);
/// node_tree.tick(&engine, 0.016);
///
/// let list_view = node_tree[list_id].get_component::<UIListView>().expect("list view");
/// assert_eq!(list_view.get_item_size(), 30.0);
/// assert_eq!(list_view.get_active_item_count(), 5);
/// assert_eq!(*bound_index_list.borrow(), vec![0, 1, 2, 3, 4]);
/// let fourth_id = list_view.get_item_node(4).expect("item 4");
/// assert_eq!(node_tree[fourth_id].get_local_position(), Vector2::create(0.0, 120.0));
/// assert!(list_view.get_item_node(100).is_none());
/// ```
//==============================================================================
pub struct UIListView {
	component_base: ComponentBase,
	create_item_event: Option<UIListViewCreateItemEvent>,
	item_count: usize,
	item_size: f32,
	spacing: f32,
	is_horizontal: bool,
	buffer_item_count: usize,
	bind_item_event: Option<UIListViewBindItemEvent>,
	reach_end_event: Option<UIListViewReachEndEvent>,
	reach_end_threshold: f32,
	is_reach_end_armed: bool,
	active_item_list: Vec<(usize, NodeId)>,
	free_item_list: Vec<NodeId>,
	is_scroll_content_size_pending: bool,
	is_refresh_pending: bool,
	/// 진행 방향이 바뀌어 보이는 항목을 다음 tick 에 회수하고 새 축에 다시 놓을지.
	is_item_layout_pending: bool,
}

#[crate::reflect]
impl UIListView {
	//==============================================================================
	/// 생성. (세로, 여유 1 칸, 끝 알림 거리 120 픽셀)
	/// - 노드에 붙일 때는 node_tree.add_component 를 쓴다. (노드에 스크롤 뷰가 없으면 UIScrollView 를 함께 붙인다)
	///
	/// # 돌려줌
	/// 항목 0 개, 항목 크기 0(처음 만든 항목에서 읽음), 간격 0 의 새 리스트 뷰.
	//==============================================================================
	pub fn new() -> UIListView {
		UIListView {
			component_base: ComponentBase::new(),
			create_item_event: None,
			item_count: 0,
			item_size: 0.0,
			spacing: 0.0,
			is_horizontal: false,
			buffer_item_count: 1,
			bind_item_event: None,
			reach_end_event: None,
			reach_end_threshold: 120.0,
			is_reach_end_armed: true,
			active_item_list: Vec::new(),
			free_item_list: Vec::new(),
			is_scroll_content_size_pending: false,
			is_refresh_pending: false,
			is_item_layout_pending: false,
		}
	}

	//==============================================================================
	/// 항목 노드 만들기 함수 설정. (원본 setTemplateNode 자리)
	/// # 인자
	/// - `create_item_event`: 트리에 새 항목 노드를 지어 돌려주는 함수. 돌려준 노드는 content 노드의 자식으로 옮겨 붙인다. 트리에 없는 노드를 돌려주면 그 tick 의 항목 만들기를 멈춘다.
	//==============================================================================
	pub fn set_create_item_event(&mut self, create_item_event: UIListViewCreateItemEvent) {
		self.create_item_event = Some(create_item_event);
	}

	//==============================================================================
	/// 템플릿 노드 설정. (UIDocument 로 직렬화해 두고 항목마다 복제한다, 템플릿은 끈다. 항목 크기가 0 이면 템플릿 크기로, 원본 setTemplateNode)
	/// - UIDocument 에 등록한 엔진 종류만 복제한다. 게임 컴포넌트가 든 템플릿은 set_template_node_with_document 를 쓴다.
	/// - 이 위젯을 트리에서 꺼내 불러야 해서 씬 코드에서는 node_tree.with_component_mut 안에서 부른다.
	///
	/// # 인자
	/// - `node_tree`: 템플릿 노드가 든 트리.
	/// - `template_node_id`: 복제할 템플릿 노드. 없는 노드면 아무것도 하지 않는다.
	///
	/// # 예제
	/// 템플릿 노드를 항목마다 복제하기
	/// ```rust
	/// use vanilla::{Engine, EngineConfiguration, NodeTree, Pivot, UIListView, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let list_id = node_tree.create_child(root_id, "list");
	/// node_tree[list_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[list_id].set_content_size(Vector2::create(200.0, 100.0));
	/// node_tree.add_component(list_id, UIListView::new());
	/// let template_id = node_tree.create_child(root_id, "template");
	/// node_tree[template_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[template_id].set_content_size(Vector2::create(200.0, 40.0));
	///
	/// node_tree.with_component_mut::<UIListView, _>(list_id, |list_view, tree| {
	/// 	list_view.set_template_node(tree, template_id);
	/// 	list_view.set_item_count(20);
	/// });
	/// assert!(!node_tree[template_id].is_active());
	/// node_tree.tick(&engine, 0.016);
	///
	/// let list_view = node_tree[list_id].get_component::<UIListView>().expect("list view");
	/// assert_eq!(list_view.get_item_size(), 40.0);
	/// assert_eq!(list_view.get_active_item_count(), 4);
	/// let item_id = list_view.get_item_node(1).expect("item 1");
	/// assert_eq!(node_tree[item_id].get_content_size(), Vector2::create(200.0, 40.0));
	/// assert!(node_tree[item_id].is_active());
	/// ```
	//==============================================================================
	pub fn set_template_node(&mut self, node_tree: &mut NodeTree, template_node_id: NodeId) {
		self.set_template_node_with_document(node_tree, template_node_id, UIDocument::new());
	}

	//==============================================================================
	/// 템플릿 노드 설정. (게임 컴포넌트를 등록한 UIDocument 로 직렬화하고 복제한다)
	/// # 인자
	/// - `node_tree`: 템플릿 노드가 든 트리.
	/// - `template_node_id`: 복제할 템플릿 노드. 없는 노드면 아무것도 하지 않는다.
	/// - `ui_document`: 템플릿을 쓰고 읽을 문서. (템플릿의 컴포넌트 종류를 등록해 둔 것, 항목을 만들 때마다 이 문서로 읽는다)
	//==============================================================================
	pub fn set_template_node_with_document(&mut self, node_tree: &mut NodeTree, template_node_id: NodeId, ui_document: UIDocument) {
		let mut template_data = ui_document.serialize_node(node_tree, template_node_id);
		if let Some(template_object) = template_data.as_object_mut() {
			template_object.insert(String::from("active"), serde_json::Value::Bool(true));
		}
		let template_size = match node_tree.get_node_mut(template_node_id) {
			Some(template_node) => {
				template_node.set_active(false);
				template_node.get_content_size()
			}
			None => {
				return;
			}
		};
		if self.item_size <= 0.0 {
			self.item_size = self.select_axis(template_size);
		}
		let mut ui_document = ui_document;
		self.create_item_event = Some(Box::new(move |tree: &mut NodeTree| ui_document.deserialize_node(tree, &template_data)));
		self.is_scroll_content_size_pending = true;
	}

	//==============================================================================
	/// 항목 수 설정. (늘면 끝 알림을 다시 무장한다. 스크롤 영역 갱신과 다시 바인딩은 다음 tick)
	/// # 인자
	/// - `item_count`: 전체 항목 수. (줄면 넘친 항목 노드는 다음 tick 에 회수한다)
	//==============================================================================
	pub fn set_item_count(&mut self, item_count: usize) {
		let previous_item_count = self.item_count;
		self.item_count = item_count;
		if self.item_count > previous_item_count {
			self.is_reach_end_armed = true;
		}
		self.is_scroll_content_size_pending = true;
		self.is_refresh_pending = true;
	}

	//==============================================================================
	/// 보이는 항목 모두 다시 바인딩. (데이터 내용이 바뀌었을 때. 다음 tick 에 한다)
	//==============================================================================
	pub fn refresh(&mut self) {
		self.is_refresh_pending = true;
	}

	//==============================================================================
	/// 보이는 항목 모두 지금 다시 바인딩. (트리를 쥔 씬 코드용. 원본 refresh 그대로)
	/// - 바인드 이벤트가 없으면 아무것도 하지 않는다. 항목 수를 넘은 항목과 지워진 노드는 건너뛴다.
	///
	/// # 인자
	/// - `node_tree`: 항목 노드가 든 트리.
	//==============================================================================
	pub fn refresh_in_tree(&mut self, node_tree: &mut NodeTree) {
		self.is_refresh_pending = false;
		let item_count = self.item_count;
		let bind_item_event = match self.bind_item_event.as_mut() {
			Some(bind_item_event) => {
				bind_item_event
			}
			None => {
				return;
			}
		};
		for (item_index, item_node_id) in self.active_item_list.iter().copied() {
			if item_index >= item_count || !node_tree.contains(item_node_id) {
				continue;
			}
			bind_item_event(node_tree, item_node_id, item_index);
		}
	}

	//==============================================================================
	/// 특정 항목의 실제 노드. (화면 밖이면 None)
	/// # 인자
	/// - `item_index`: 항목 번호. (0 부터)
	///
	/// # 돌려줌
	/// 그 항목을 보이는 노드. 지금 만들어 둔 범위(보이는 범위와 여유 칸) 밖이면 None.
	//==============================================================================
	pub fn get_item_node(&self, item_index: usize) -> Option<NodeId> {
		self.active_item_list.iter().find(|(active_index, _)| *active_index == item_index).map(|(_, item_node_id)| *item_node_id)
	}

	//==============================================================================
	/// 바인드 이벤트 설정.
	/// # 인자
	/// - `bind_item_event`: (트리, 항목 노드, 항목 번호) 를 받아 항목 노드 내용을 채우는 함수. 노드를 새로 놓을 때(만들거나 다시 쓸 때)와 다시 바인딩할 때 부른다.
	//==============================================================================
	pub fn set_bind_item_event(&mut self, bind_item_event: UIListViewBindItemEvent) {
		self.bind_item_event = Some(bind_item_event);
	}

	//==============================================================================
	/// 끝 알림 설정.
	/// # 인자
	/// - `reach_end_event`: 끝까지 남은 길이가 끝 알림 거리 이하가 되면 한 번 부를 함수. (리스트 뷰와 문맥을 받는다, 안에서 set_item_count 로 항목을 늘리면 다시 무장된다)
	///
	/// # 예제
	/// 끝 근처에 닿으면 항목 50 개 더 불러오기
	/// ```rust
	/// use std::cell::Cell;
	/// use std::rc::Rc;
	/// use vanilla::{ComponentContext, Engine, EngineConfiguration, NodeTree, Pivot, UIListView, UIScrollView, Vector2};
	///
	/// let engine = Engine::new(EngineConfiguration::new());
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let list_id = node_tree.create_child(root_id, "list");
	/// node_tree[list_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[list_id].set_content_size(Vector2::create(200.0, 100.0));
	/// let reach_end_count = Rc::new(Cell::new(0));
	/// let reach_end_counter = reach_end_count.clone();
	///
	/// let list_view = node_tree.add_component(list_id, UIListView::new()).expect("list view");
	/// list_view.set_create_item_event(Box::new(|tree: &mut NodeTree| {
	/// 	tree.create_node("item")
	/// }));
	/// list_view.set_item_size(30.0);
	/// list_view.set_item_count(100);
	/// list_view.set_reach_end_event(Box::new(move |list_view: &mut UIListView, _context: &mut ComponentContext| {
	/// 	reach_end_counter.set(reach_end_counter.get() + 1);
	/// 	let item_count = list_view.get_item_count();
	/// 	list_view.set_item_count(item_count + 50);
	/// }));
	/// node_tree.tick(&engine, 0.016);
	/// assert_eq!(reach_end_count.get(), 0);
	///
	/// node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::create(0.0, -2900.0));
	/// node_tree.tick(&engine, 0.016);
	/// assert_eq!(reach_end_count.get(), 1);
	/// assert_eq!(node_tree[list_id].get_component::<UIListView>().expect("list view").get_item_count(), 150);
	/// ```
	//==============================================================================
	pub fn set_reach_end_event(&mut self, reach_end_event: UIListViewReachEndEvent) {
		self.reach_end_event = Some(reach_end_event);
	}

	//==============================================================================
	/// 끝 알림 거리 설정. (픽셀)
	/// # 인자
	/// - `reach_end_threshold`: 끝까지 남은 길이가 이 값 이하면 알린다. (픽셀, 기본 120)
	//==============================================================================
	pub fn set_reach_end_threshold(&mut self, reach_end_threshold: f32) {
		self.reach_end_threshold = reach_end_threshold;
	}

	//==============================================================================
	/// 진행 축 방향 항목 크기 설정. (스크롤 영역은 다음 tick 에 갱신)
	/// # 인자
	/// - `item_size`: 항목 하나의 진행 축 길이. (픽셀, 0 이하면 처음 만든 항목 노드의 content_size 를 읽는다)
	//==============================================================================
	pub fn set_item_size(&mut self, item_size: f32) {
		self.item_size = item_size;
		self.is_scroll_content_size_pending = true;
	}

	//==============================================================================
	/// 항목 사이 간격 설정. (스크롤 영역은 다음 tick 에 갱신)
	/// # 인자
	/// - `spacing`: 항목 사이 간격. (픽셀, 기본 0)
	//==============================================================================
	pub fn set_spacing(&mut self, spacing: f32) {
		self.spacing = spacing;
		self.is_scroll_content_size_pending = true;
	}

	//==============================================================================
	/// 가로 진행 설정. (기본 세로)
	/// - 바뀌면 다음 tick 에 스크롤 영역을 다시 재고, 보이는 항목을 새 축에 다시 놓는다. (항목 크기를 읽기 전에 정한다, 스크롤 뷰의 축은 따로 정한다)
	///
	/// # 인자
	/// - `is_horizontal`: true 면 항목을 x 축으로 늘어놓는다.
	//==============================================================================
	pub fn set_horizontal(&mut self, is_horizontal: bool) {
		if self.is_horizontal == is_horizontal {
			return;
		}
		self.is_horizontal = is_horizontal;
		self.is_scroll_content_size_pending = true;
		self.is_item_layout_pending = true;
	}

	//==============================================================================
	/// 화면 밖 여유 항목 수 설정.
	/// # 인자
	/// - `buffer_item_count`: 보이는 범위 앞뒤로 더 만들어 둘 항목 수. (기본 1)
	//==============================================================================
	pub fn set_buffer_item_count(&mut self, buffer_item_count: usize) {
		self.buffer_item_count = buffer_item_count;
	}

	//==============================================================================
	/// 항목 수 반환.
	/// # 돌려줌
	/// 전체 항목 수.
	//==============================================================================
	pub fn get_item_count(&self) -> usize {
		self.item_count
	}

	//==============================================================================
	/// 항목 크기 반환.
	/// # 돌려줌
	/// 항목 하나의 진행 축 길이. (픽셀, 아직 읽지 않았으면 0)
	//==============================================================================
	pub fn get_item_size(&self) -> f32 {
		self.item_size
	}

	//==============================================================================
	/// 항목 사이 간격 반환.
	/// # 돌려줌
	/// 항목 사이 간격. (픽셀)
	//==============================================================================
	pub fn get_spacing(&self) -> f32 {
		self.spacing
	}

	//==============================================================================
	/// 가로 진행 여부.
	/// # 돌려줌
	/// 가로로 늘어놓으면 true.
	//==============================================================================
	pub fn is_horizontal(&self) -> bool {
		self.is_horizontal
	}

	//==============================================================================
	/// 지금 만들어 둔(보이는) 항목 수.
	/// # 돌려줌
	/// 지금 노드로 만들어 둔 항목 수. (보이는 범위와 여유 칸)
	//==============================================================================
	pub fn get_active_item_count(&self) -> usize {
		self.active_item_list.len()
	}

	//==============================================================================
	/// 진행 축의 값 고르기.
	//==============================================================================
	fn select_axis(&self, vector: Vector2) -> f32 {
		if self.is_horizontal {
			vector.x
		}
		else {
			vector.y
		}
	}

	//==============================================================================
	/// 스크롤 영역 크기 계산. (진행 축 = 항목 수 x 간격 포함 크기 - 마지막 간격, 다른 축 = 호스트 크기)
	//==============================================================================
	fn calculate_scroll_content_size(&self, host_content_size: Vector2) -> Vector2 {
		let pitch = self.item_size + self.spacing;
		let total_length = if self.item_count > 0 {
			self.item_count as f32 * pitch - self.spacing
		}
		else {
			0.0
		};
		if self.is_horizontal {
			Vector2::create(total_length, host_content_size.y)
		}
		else {
			Vector2::create(host_content_size.x, total_length)
		}
	}

	//==============================================================================
	/// 항목 노드 확보. (회수분을 다시 쓰고, 없으면 만들어서 content 노드에 붙인다)
	//==============================================================================
	fn acquire_item_node(&mut self, node_tree: &mut NodeTree, host_id: NodeId, content_id: Option<NodeId>) -> Option<NodeId> {
		// 밖에서 지운 노드는 버린다.
		while let Some(free_item_node_id) = self.free_item_list.pop() {
			if node_tree.contains(free_item_node_id) {
				return Some(free_item_node_id);
			}
		}
		let create_item_event = self.create_item_event.as_mut()?;
		let item_node_id = create_item_event(node_tree);
		if !node_tree.contains(item_node_id) {
			return None;
		}
		let parent_id = content_id.unwrap_or(host_id);
		node_tree.add_child(parent_id, item_node_id);
		Some(item_node_id)
	}

	//==============================================================================
	/// 항목 크기를 처음 만든 항목 노드에서 읽는다. (원본이 템플릿 content_size 를 읽는 자리. 노드는 회수 목록에 넣어 다시 쓴다)
	//==============================================================================
	fn measure_item_size(&mut self, node_tree: &mut NodeTree, host_id: NodeId, content_id: Option<NodeId>) {
		let item_node_id = match self.acquire_item_node(node_tree, host_id, content_id) {
			Some(item_node_id) => {
				item_node_id
			}
			None => {
				return;
			}
		};
		let item_node = &mut node_tree[item_node_id];
		let item_content_size = item_node.get_content_size();
		item_node.set_active(false);
		self.item_size = self.select_axis(item_content_size);
		self.free_item_list.push(item_node_id);
		self.is_scroll_content_size_pending = true;
	}

	//==============================================================================
	/// 끝 알림 부르기. (부르는 동안 칸에서 꺼냈다가, 콜백이 새로 정하지 않았으면 되돌린다)
	//==============================================================================
	fn fire_reach_end_event(&mut self, context: &mut ComponentContext) {
		let mut reach_end_event = match self.reach_end_event.take() {
			Some(reach_end_event) => {
				reach_end_event
			}
			None => {
				return;
			}
		};
		reach_end_event(self, context);
		if self.reach_end_event.is_none() {
			self.reach_end_event = Some(reach_end_event);
		}
	}
}

impl Default for UIListView {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> UIListView {
		UIListView::new()
	}
}

#[crate::reflect]
impl Component for UIListView {
	//==============================================================================
	/// 컴포넌트 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 종류 이름. ("UIListView")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIListView"
	}

	//==============================================================================
	/// 트리에 붙음. (노드에 스크롤 뷰가 없으면 UIScrollView 를 먼저 붙인다. 원본 require)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		let has_scroll_view = node_tree[node_id].has_component::<UIScrollView>() || node_tree[node_id].has_component::<UISnapScrollView>();
		if !has_scroll_view {
			node_tree.add_component(node_id, UIScrollView::new());
		}
	}

	//==============================================================================
	/// 갱신. (미뤄 둔 스크롤 영역 / 다시 바인딩 → 보이는 범위 계산 → 회수 / 생성 → 끝 알림)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		if self.create_item_event.is_none() {
			return;
		}
		let host_id = context.get_node_id();
		let host_content_size = context.get_transform().get_content_size();
		let content_id = match find_scroll_view(context.get_node()) {
			Some(scroll_view) => {
				scroll_view.get_content()
			}
			None => {
				return;
			}
		};

		// 진행 방향이 바뀌었으면 보이는 항목을 모두 회수해 새 축에 다시 놓는다.
		if self.is_item_layout_pending {
			self.is_item_layout_pending = false;
			let node_tree = context.get_node_tree_mut();
			for (_, item_node_id) in self.active_item_list.drain(..) {
				if let Some(item_node) = node_tree.get_node_mut(item_node_id) {
					item_node.set_active(false);
					self.free_item_list.push(item_node_id);
				}
			}
		}

		// 항목 크기를 정하지 않았으면 처음 만든 항목에서 읽는다.
		if self.item_size <= 0.0 {
			self.measure_item_size(context.get_node_tree_mut(), host_id, content_id);
		}

		// 미뤄 둔 스크롤 영역 갱신, 그리고 지금 오프셋.
		let scroll_content_size = self.calculate_scroll_content_size(host_content_size);
		let is_scroll_content_size_pending = self.is_scroll_content_size_pending;
		self.is_scroll_content_size_pending = false;
		let scroll_offset = match find_scroll_view_mut(context.get_node_mut()) {
			Some(scroll_view) => {
				if is_scroll_content_size_pending {
					scroll_view.set_scroll_content_size(scroll_content_size);
				}
				scroll_view.get_scroll_offset()
			}
			None => {
				return;
			}
		};

		// 미뤄 둔 다시 바인딩.
		if self.is_refresh_pending {
			self.refresh_in_tree(context.get_node_tree_mut());
		}

		let view_length = self.select_axis(host_content_size);
		let scrolled_length = -self.select_axis(scroll_offset);
		let pitch = self.item_size + self.spacing;
		if pitch <= 0.0 {
			return;
		}

		// 보이는 범위. (여유 칸 포함)
		let buffer_item_count = self.buffer_item_count as i64;
		let first_index = ((scrolled_length / pitch).floor() as i64 - buffer_item_count).max(0);
		let last_index = (((scrolled_length + view_length) / pitch).ceil() as i64 + buffer_item_count - 1).min(self.item_count as i64 - 1);

		// 범위를 벗어난 항목 회수.
		let node_tree = context.get_node_tree_mut();
		let free_item_list = &mut self.free_item_list;
		self.active_item_list.retain(|(item_index, item_node_id)| {
			let item_index = *item_index as i64;
			if item_index >= first_index && item_index <= last_index {
				return true;
			}
			if let Some(item_node) = node_tree.get_node_mut(*item_node_id) {
				item_node.set_active(false);
				free_item_list.push(*item_node_id);
			}
			false
		});

		// 빠진 항목 생성 / 재사용.
		let mut item_index = first_index;
		while item_index <= last_index {
			let current_index = item_index as usize;
			item_index += 1;
			if self.get_item_node(current_index).is_some() {
				continue;
			}
			let item_node_id = match self.acquire_item_node(node_tree, host_id, content_id) {
				Some(item_node_id) => {
					item_node_id
				}
				None => {
					break;
				}
			};
			let item_position = if self.is_horizontal {
				Vector2::create(current_index as f32 * pitch, 0.0)
			}
			else {
				Vector2::create(0.0, current_index as f32 * pitch)
			};
			let item_name = format!("Item{}", current_index);
			let item_node = &mut node_tree[item_node_id];
			item_node.set_name(&item_name);
			item_node.set_active(true);
			item_node.set_local_position(item_position);
			self.active_item_list.push((current_index, item_node_id));
			if let Some(bind_item_event) = self.bind_item_event.as_mut() {
				bind_item_event(node_tree, item_node_id, current_index);
			}
		}

		// 끝 근처 알림. (무한 스크롤)
		if self.reach_end_event.is_some() && self.is_reach_end_armed && self.item_count > 0 {
			let total_length = self.item_count as f32 * pitch - self.spacing;
			let remain_length = total_length - (scrolled_length + view_length);
			if remain_length <= self.reach_end_threshold {
				self.is_reach_end_armed = false;
				self.fire_reach_end_event(context);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::base::pivot::Pivot;
	use crate::core::engine::{Engine, EngineConfiguration};
	use std::cell::{Cell, RefCell};
	use std::rc::Rc;

	//==============================================================================
	/// 시험 장면. (왼쪽 위 (0, 0), 200 x 100 인 리스트, 항목은 200 x 30. 만든 수를 센다)
	//==============================================================================
	fn create_list_tree(created_count: &Rc<Cell<u32>>, bound_index_list: &Rc<RefCell<Vec<usize>>>) -> (NodeTree, NodeId) {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let list_id = node_tree.create_child(root_id, "list");
		let list_node = &mut node_tree[list_id];
		list_node.set_pivot(Pivot::TOP_LEFT);
		list_node.set_content_size(Vector2::create(200.0, 100.0));
		let created_counter = created_count.clone();
		let bound_index_recorder = bound_index_list.clone();
		let list_view = node_tree.add_component(list_id, UIListView::new()).expect("list view");
		list_view.set_create_item_event(Box::new(move |tree: &mut NodeTree| {
			created_counter.set(created_counter.get() + 1);
			let item_id = tree.create_node("item");
			let item_node = &mut tree[item_id];
			item_node.set_pivot(Pivot::TOP_LEFT);
			item_node.set_content_size(Vector2::create(200.0, 30.0));
			item_id
		}));
		list_view.set_bind_item_event(Box::new(move |_tree: &mut NodeTree, _item_id: NodeId, item_index: usize| {
			bound_index_recorder.borrow_mut().push(item_index);
		}));
		list_view.set_item_count(100);
		(node_tree, list_id)
	}

	//==============================================================================
	/// 리스트 뷰 반환.
	//==============================================================================
	fn get_list_view(node_tree: &mut NodeTree, list_id: NodeId) -> &mut UIListView {
		node_tree[list_id].get_component_mut::<UIListView>().expect("list view")
	}

	//==============================================================================
	/// 붙으면 UIScrollView 가 함께 붙고, 첫 tick 에 항목 크기와 스크롤 영역이 정해져 보이는 범위만 만든다.
	//==============================================================================
	#[test]
	fn creates_only_visible_range() {
		let engine = Engine::new(EngineConfiguration::new());
		let created_count = Rc::new(Cell::new(0));
		let bound_index_list = Rc::new(RefCell::new(Vec::new()));
		let (mut node_tree, list_id) = create_list_tree(&created_count, &bound_index_list);
		assert!(node_tree[list_id].has_component::<UIScrollView>());
		node_tree.tick(&engine, 0.016);
		assert_eq!(get_list_view(&mut node_tree, list_id).get_item_size(), 30.0);
		assert_eq!(node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_content_size(), Vector2::create(200.0, 3000.0));
		// 0 ~ 100 픽셀 + 여유 1 칸: 0 ~ 4 번.
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 5);
		assert_eq!(created_count.get(), 5);
		assert_eq!(*bound_index_list.borrow(), vec![0, 1, 2, 3, 4]);
		let fourth_id = get_list_view(&mut node_tree, list_id).get_item_node(4).expect("item 4");
		assert_eq!(node_tree[fourth_id].get_local_position(), Vector2::create(0.0, 120.0));
		assert_eq!(node_tree[fourth_id].get_name(), "Item4");
		let content_id = node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_content().expect("content");
		assert_eq!(node_tree.get_parent_id(fourth_id), Some(content_id));
	}

	//==============================================================================
	/// 스크롤하면 벗어난 항목을 회수해 다시 쓴다. (노드 수는 화면 분량에 머문다)
	//==============================================================================
	#[test]
	fn scrolling_recycles_items() {
		let engine = Engine::new(EngineConfiguration::new());
		let created_count = Rc::new(Cell::new(0));
		let bound_index_list = Rc::new(RefCell::new(Vec::new()));
		let (mut node_tree, list_id) = create_list_tree(&created_count, &bound_index_list);
		node_tree.tick(&engine, 0.016);
		node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::create(0.0, -600.0));
		node_tree.tick(&engine, 0.016);
		// 600 ~ 700 픽셀 + 여유: 19 ~ 24 번.
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 6);
		assert!(get_list_view(&mut node_tree, list_id).get_item_node(0).is_none());
		let nineteenth_id = get_list_view(&mut node_tree, list_id).get_item_node(19).expect("item 19");
		assert_eq!(node_tree[nineteenth_id].get_local_position(), Vector2::create(0.0, 570.0));
		assert!(node_tree[nineteenth_id].is_active());
		assert_eq!(created_count.get(), 6);

		// 끝까지 갔다가 처음으로 돌아와도 만든 노드 수는 늘지 않는다.
		for offset_step in 0..30 {
			let offset_y = -(offset_step as f32) * 100.0;
			node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::create(0.0, offset_y));
			node_tree.tick(&engine, 0.016);
		}
		node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::zero());
		node_tree.tick(&engine, 0.016);
		assert!(created_count.get() <= 8);
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 5);
	}

	//==============================================================================
	/// 끝 근처에서 알림이 한 번 오고, 항목 수가 늘면 다시 무장된다. 항목 수를 줄이면 넘친 항목은 회수된다.
	//==============================================================================
	#[test]
	fn reach_end_fires_once_and_rearms() {
		let engine = Engine::new(EngineConfiguration::new());
		let created_count = Rc::new(Cell::new(0));
		let bound_index_list = Rc::new(RefCell::new(Vec::new()));
		let (mut node_tree, list_id) = create_list_tree(&created_count, &bound_index_list);
		let reach_end_count = Rc::new(Cell::new(0));
		let reach_end_counter = reach_end_count.clone();
		get_list_view(&mut node_tree, list_id).set_reach_end_event(Box::new(move |list_view: &mut UIListView, _context: &mut ComponentContext| {
			reach_end_counter.set(reach_end_counter.get() + 1);
			// 알림 안에서 항목을 더 불러온다.
			let item_count = list_view.get_item_count();
			list_view.set_item_count(item_count + 50);
		}));
		node_tree.tick(&engine, 0.016);
		assert_eq!(reach_end_count.get(), 0);
		node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::create(0.0, -2900.0));
		node_tree.tick(&engine, 0.016);
		assert_eq!(reach_end_count.get(), 1);
		assert_eq!(get_list_view(&mut node_tree, list_id).get_item_count(), 150);
		node_tree.tick(&engine, 0.016);
		assert_eq!(reach_end_count.get(), 1);
		assert_eq!(node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_content_size(), Vector2::create(200.0, 4500.0));

		// 항목 수를 줄이면 그 범위 밖 항목은 회수되고, 남은 항목은 다시 바인딩된다.
		bound_index_list.borrow_mut().clear();
		node_tree[list_id].get_component_mut::<UIScrollView>().expect("scroll view").set_scroll_offset(Vector2::zero());
		node_tree.tick(&engine, 0.016);
		get_list_view(&mut node_tree, list_id).set_item_count(3);
		bound_index_list.borrow_mut().clear();
		node_tree.tick(&engine, 0.016);
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 3);
		assert_eq!(*bound_index_list.borrow(), vec![0, 1, 2]);
	}

	//==============================================================================
	/// 가로 리스트는 x 축으로 늘어선다. 간격을 주면 간격만큼 벌어진다.
	//==============================================================================
	#[test]
	fn horizontal_list_with_spacing() {
		let engine = Engine::new(EngineConfiguration::new());
		let created_count = Rc::new(Cell::new(0));
		let bound_index_list = Rc::new(RefCell::new(Vec::new()));
		let (mut node_tree, list_id) = create_list_tree(&created_count, &bound_index_list);
		{
			let list_view = get_list_view(&mut node_tree, list_id);
			list_view.set_horizontal(true);
			list_view.set_item_size(50.0);
			list_view.set_spacing(10.0);
			list_view.set_item_count(10);
		}
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_content_size(), Vector2::create(590.0, 100.0));
		// 0 ~ 200 픽셀, 간격 포함 60: ceil(200 / 60) = 4 → 0 ~ 4 번.
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 5);
		let second_id = get_list_view(&mut node_tree, list_id).get_item_node(2).expect("item 2");
		assert_eq!(node_tree[second_id].get_local_position(), Vector2::create(120.0, 0.0));
	}

	//==============================================================================
	/// 붙은 뒤 진행 방향을 바꾸면 스크롤 영역을 다시 재고 보이는 항목을 새 축에 다시 놓는다.
	//==============================================================================
	#[test]
	fn switching_direction_updates_scroll_area_and_items() {
		let engine = Engine::new(EngineConfiguration::new());
		let created_count = Rc::new(Cell::new(0));
		let bound_index_list = Rc::new(RefCell::new(Vec::new()));
		let (mut node_tree, list_id) = create_list_tree(&created_count, &bound_index_list);
		{
			let list_view = get_list_view(&mut node_tree, list_id);
			list_view.set_item_size(50.0);
			list_view.set_item_count(10);
		}
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_content_size(), Vector2::create(200.0, 500.0));
		let vertical_created_count = created_count.get();
		get_list_view(&mut node_tree, list_id).set_horizontal(true);
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[list_id].get_component::<UIScrollView>().expect("scroll view").get_scroll_content_size(), Vector2::create(500.0, 100.0));
		assert_eq!(get_list_view(&mut node_tree, list_id).get_active_item_count(), 5);
		let second_id = get_list_view(&mut node_tree, list_id).get_item_node(2).expect("item 2");
		assert_eq!(node_tree[second_id].get_local_position(), Vector2::create(100.0, 0.0));
		assert!(node_tree[second_id].is_active());
		assert_eq!(created_count.get(), vertical_created_count.max(5));
	}
}
