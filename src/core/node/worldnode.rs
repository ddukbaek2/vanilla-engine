//==============================================================================
//! 월드 노드.
//! - 부모/자식 계층 + 활성화 (원본 Node)
//! - 컴포넌트 보유 + tick (원본 ComponentNode)
//! - 로컬 TRS + 렌더 재귀 (원본 TransformNode)
//! - pivot / anchor / content_size + 터치 (원본 WorldNode)
//! - 노드는 씬의 NodeTree 저장소에 들어 있고, 부모와 자식은 핸들(NodeId)로 잇는다.
//!   트리를 바꾸는 일(자식 붙이기, 지우기, 찾기)과 트리를 도는 일(tick, draw, 터치)은 NodeTree 가 한다.
//==============================================================================
use crate::base::color::Color;
use crate::base::identifier;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::rect::Rect;
use crate::base::transformmatrix::TransformMatrix;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, RequiredComponent};
use std::any::Any;
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::NodeId;

//==============================================================================
/// 노드 트랜스폼. (컴포넌트가 tick / draw 에서 받아 읽고 바꾸는 노드 상태)
/// - local_position 은 부모 영역 안에서 자기 피봇 점의 좌표다. (anchor 는 anchored_position 계산용)
/// - local_rotation 은 도 단위. (원본과 같음)
///
/// # 예제
/// 가운데 피봇 노드를 사각형에 놓고 경계 보기
/// ```rust
/// use vanilla::{NodeTransform, Pivot, Rect, Vector2};
///
/// let mut transform = NodeTransform::new();
/// transform.set_pivot(Pivot::MIDDLE_CENTER);
/// transform.set_rect(10.0, 20.0, 100.0, 50.0);
/// assert_eq!(transform.get_local_position(), Vector2::create(60.0, 45.0));
/// assert_eq!(transform.get_local_bounds(), Rect::create(10.0, 20.0, 100.0, 50.0));
/// transform.set_local_scale(Vector2::create(2.0, 2.0));
/// assert_eq!(transform.get_local_bounds(), Rect::create(-40.0, -5.0, 200.0, 100.0));
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct NodeTransform {
	local_position: Vector2,
	local_scale: Vector2,
	local_rotation: f64,
	local_opacity: f64,
	pivot: Vector2,
	content_size: Vector2,
	anchor: Vector2,
	is_visible: bool,
}

impl NodeTransform {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 위치 (0, 0), 배율 1, 회전 0, 불투명도 1, 내용 크기 0, 피봇과 앵커는 가운데, 보이는 트랜스폼.
	//==============================================================================
	pub fn new() -> NodeTransform {
		NodeTransform {
			local_position: Vector2::zero(),
			local_scale: Vector2::one(),
			local_rotation: 0.0,
			local_opacity: 1.0,
			pivot: Pivot::MIDDLE_CENTER,
			content_size: Vector2::zero(),
			anchor: Pivot::MIDDLE_CENTER,
			is_visible: true,
		}
	}

	//==============================================================================
	/// 로컬 위치 설정.
	///
	/// # 인자
	/// - `local_position`: 부모 내용 영역의 왼쪽 위에서 본 피봇 점의 자리.
	//==============================================================================
	pub fn set_local_position(&mut self, local_position: Vector2) {
		self.local_position = local_position;
	}

	//==============================================================================
	/// 로컬 위치 반환.
	///
	/// # 돌려줌
	/// 로컬 위치. (부모 내용 영역의 왼쪽 위에서 본 피봇 점의 자리)
	//==============================================================================
	pub fn get_local_position(&self) -> Vector2 {
		self.local_position
	}

	//==============================================================================
	/// 로컬 크기 설정.
	///
	/// # 인자
	/// - `local_scale`: 축마다 배율. (1 이 원래 크기, 음수면 그 축으로 뒤집혀 그려진다)
	//==============================================================================
	pub fn set_local_scale(&mut self, local_scale: Vector2) {
		self.local_scale = local_scale;
	}

	//==============================================================================
	/// 로컬 크기 반환.
	///
	/// # 돌려줌
	/// 로컬 배율.
	//==============================================================================
	pub fn get_local_scale(&self) -> Vector2 {
		self.local_scale
	}

	//==============================================================================
	/// 로컬 회전 설정. (도)
	///
	/// # 인자
	/// - `local_rotation`: 회전. (도, 양수면 화면에서 시계 방향)
	//==============================================================================
	pub fn set_local_rotation(&mut self, local_rotation: f64) {
		self.local_rotation = local_rotation;
	}

	//==============================================================================
	/// 로컬 회전 반환. (도)
	///
	/// # 돌려줌
	/// 로컬 회전. (도)
	//==============================================================================
	pub fn get_local_rotation(&self) -> f64 {
		self.local_rotation
	}

	//==============================================================================
	/// 로컬 투명도 설정. (자식에게 곱해져 내려간다)
	///
	/// # 인자
	/// - `local_opacity`: 불투명도. (0 ~ 1, 밖의 값은 잘라 넣는다)
	//==============================================================================
	pub fn set_local_opacity(&mut self, local_opacity: f64) {
		self.local_opacity = math::clamp01(local_opacity);
	}

	//==============================================================================
	/// 로컬 투명도 반환.
	///
	/// # 돌려줌
	/// 로컬 불투명도. (0 ~ 1)
	//==============================================================================
	pub fn get_local_opacity(&self) -> f64 {
		self.local_opacity
	}

	//==============================================================================
	/// 피봇 설정. (content_size 안에서 기준점의 비율, 기본 가운데)
	///
	/// # 인자
	/// - `pivot`: 내용 크기 안의 기준점 비율. ((0, 0) 왼쪽 위, (1, 1) 오른쪽 아래, Pivot 상수)
	//==============================================================================
	pub fn set_pivot(&mut self, pivot: Vector2) {
		self.pivot = pivot;
	}

	//==============================================================================
	/// 피봇 반환.
	///
	/// # 돌려줌
	/// 피봇. (내용 크기 안의 비율)
	//==============================================================================
	pub fn get_pivot(&self) -> Vector2 {
		self.pivot
	}

	//==============================================================================
	/// 컨텐츠 크기 설정.
	///
	/// # 인자
	/// - `content_size`: 내용 크기. (노드 좌표, 배율을 걸기 전)
	//==============================================================================
	pub fn set_content_size(&mut self, content_size: Vector2) {
		self.content_size = content_size;
	}

	//==============================================================================
	/// 컨텐츠 크기 반환.
	///
	/// # 돌려줌
	/// 내용 크기. (노드 좌표, 배율을 걸기 전)
	//==============================================================================
	pub fn get_content_size(&self) -> Vector2 {
		self.content_size
	}

	//==============================================================================
	/// 로컬 위치 설정. (x, y 를 바로 받는다, 원본 setLocalPositionXY)
	///
	/// # 인자
	/// - `x`: 로컬 위치 x.
	/// - `y`: 로컬 위치 y.
	//==============================================================================
	pub fn set_local_position_xy(&mut self, x: f64, y: f64) {
		self.local_position = Vector2::create(x, y);
	}

	//==============================================================================
	/// 로컬 크기 설정. (x, y 를 바로 받는다, 원본 setLocalScaleXY)
	///
	/// # 인자
	/// - `x`: 가로 배율.
	/// - `y`: 세로 배율.
	//==============================================================================
	pub fn set_local_scale_xy(&mut self, x: f64, y: f64) {
		self.local_scale = Vector2::create(x, y);
	}

	//==============================================================================
	/// 컨텐츠 크기 설정. (너비, 높이를 바로 받는다, 원본 setContentSizeXY)
	///
	/// # 인자
	/// - `width`: 내용 너비.
	/// - `height`: 내용 높이.
	//==============================================================================
	pub fn set_content_size_xy(&mut self, width: f64, height: f64) {
		self.content_size = Vector2::create(width, height);
	}

	//==============================================================================
	/// 부모 영역 안의 사각형에 놓기. (x, y 는 이 노드의 왼쪽 위, 부모 내용 영역의 왼쪽 위가 원점, 원본 setRect)
	/// - 내용 크기를 width, height 로 두고, 피봇이 어디든 왼쪽 위가 (x, y) 에 오게 로컬 위치를 정한다. (x + width * pivot.x, y + height * pivot.y)
	/// - 앵커는 바꾸지 않는다. 로컬 위치는 앵커와 무관하게 부모 왼쪽 위 기준이라 앵커가 어디든 같은 자리다.
	/// - 로컬 배율과 회전은 보지 않는다. (배율 1, 회전 0 일 때 화면에서도 그 사각형이다)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x. (부모 내용 영역의 왼쪽 위 기준)
	/// - `y`: 왼쪽 위 y. (부모 내용 영역의 왼쪽 위 기준)
	/// - `width`: 내용 너비.
	/// - `height`: 내용 높이.
	//==============================================================================
	pub fn set_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		self.content_size = Vector2::create(width, height);
		self.local_position = Vector2::create(x + width * self.pivot.x, y + height * self.pivot.y);
	}

	//==============================================================================
	/// 앵커 설정. (부모 content_size 안의 비율)
	///
	/// # 인자
	/// - `anchor`: 부모 내용 크기 안의 비율. (anchored_position 의 기준점, Pivot 상수)
	//==============================================================================
	pub fn set_anchor(&mut self, anchor: Vector2) {
		self.anchor = anchor;
	}

	//==============================================================================
	/// 앵커 반환.
	///
	/// # 돌려줌
	/// 앵커. (부모 내용 크기 안의 비율)
	//==============================================================================
	pub fn get_anchor(&self) -> Vector2 {
		self.anchor
	}

	//==============================================================================
	/// 앵커 기준 위치 설정. (부모 content_size 를 알아야 한다)
	/// - 로컬 위치를 (부모 내용 크기 x 앵커) + anchored_position 으로 둔다.
	///
	/// # 인자
	/// - `anchored_position`: 앵커 점에서 본 위치.
	/// - `parent_content_size`: 부모의 내용 크기.
	///
	/// # 예제
	/// 부모 오른쪽 아래에서 20 씩 안쪽에 놓기
	/// ```rust
	/// use vanilla::{NodeTransform, Pivot, Vector2};
	///
	/// let mut transform = NodeTransform::new();
	/// transform.set_anchor(Pivot::BOTTOM_RIGHT);
	/// let parent_content_size = Vector2::create(800.0, 600.0);
	/// transform.set_anchored_position(Vector2::create(-20.0, -20.0), parent_content_size);
	/// assert_eq!(transform.get_local_position(), Vector2::create(780.0, 580.0));
	/// assert_eq!(transform.get_anchored_position(parent_content_size), Vector2::create(-20.0, -20.0));
	/// ```
	//==============================================================================
	pub fn set_anchored_position(&mut self, anchored_position: Vector2, parent_content_size: Vector2) {
		let anchor_point = parent_content_size.multiply(self.anchor);
		self.local_position = anchor_point.add(anchored_position);
	}

	//==============================================================================
	/// 앵커 기준 위치 반환.
	///
	/// # 인자
	/// - `parent_content_size`: 부모의 내용 크기.
	///
	/// # 돌려줌
	/// 앵커 점(부모 내용 크기 x 앵커)에서 본 로컬 위치.
	//==============================================================================
	pub fn get_anchored_position(&self, parent_content_size: Vector2) -> Vector2 {
		let anchor_point = parent_content_size.multiply(self.anchor);
		self.local_position.subtract(anchor_point)
	}

	//==============================================================================
	/// 보이기 설정. (꺼도 tick 은 돈다. 원본 isVisible)
	///
	/// # 인자
	/// - `is_visible`: 그릴지. (false 면 이 노드와 자손을 그리지 않고 터치도 받지 않는다)
	//==============================================================================
	pub fn set_visible(&mut self, is_visible: bool) {
		self.is_visible = is_visible;
	}

	//==============================================================================
	/// 보이기 여부.
	///
	/// # 돌려줌
	/// 보이면 true.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.is_visible
	}

	//==============================================================================
	/// 로컬 변환 행렬. (push_transform 과 같은 순서: 이동 → 회전 → 크기 → 피봇 보정)
	///
	/// # 돌려줌
	/// 이 노드 내용 좌표의 점을 부모 내용 좌표로 옮기는 행렬.
	//==============================================================================
	pub fn calculate_local_matrix(&self) -> TransformMatrix {
		let mut local_matrix = TransformMatrix::identity();
		local_matrix.translate(self.local_position.x, self.local_position.y);
		local_matrix.rotate(math::degree_to_radian(self.local_rotation));
		local_matrix.scale(self.local_scale.x, self.local_scale.y);
		local_matrix.translate(-self.content_size.x * self.pivot.x, -self.content_size.y * self.pivot.y);
		local_matrix
	}

	//==============================================================================
	/// 부모 영역에서 본 경계 사각형. (회전이 없을 때 정확하다. 충돌 검사 등)
	///
	/// # 돌려줌
	/// 부모 내용 좌표의 경계 사각형. (배율은 절댓값으로 걸고 회전은 넣지 않는다)
	//==============================================================================
	pub fn get_local_bounds(&self) -> Rect {
		let width = self.content_size.x * self.local_scale.x.abs();
		let height = self.content_size.y * self.local_scale.y.abs();
		Rect::create(self.local_position.x - width * self.pivot.x, self.local_position.y - height * self.pivot.y, width, height)
	}
}

impl Default for NodeTransform {
	//==============================================================================
	/// 기본값. (NodeTransform::new 와 같다)
	//==============================================================================
	fn default() -> NodeTransform {
		NodeTransform::new()
	}
}


//==============================================================================
/// 월드 노드.
/// - 이름, 활성, 터치 받기, 트랜스폼, 컴포넌트 목록, 부모와 자식 핸들을 든다.
///
/// # 예제
/// 노드 하나에 컴포넌트를 붙이고 찾기
/// ```rust
/// use vanilla::{Color, Paint, Vector2, WorldNode};
///
/// let mut node = WorldNode::new("card");
/// node.set_content_size(Vector2::create(120.0, 80.0));
/// node.add_component(Paint::create_with_color(Color::white()));
/// assert_eq!(node.get_name(), "card");
/// assert!(node.has_component::<Paint>());
/// assert_eq!(node.get_component::<Paint>().expect("paint").get_color(), Color::white());
/// assert!(node.is_root());
/// ```
//==============================================================================
pub struct WorldNode {
	instance_id: u64,
	name: String,
	is_active: bool,
	is_interactable: bool,
	is_gizmo_visible: bool,
	is_opacity_layer: bool,
	transform: NodeTransform,
	pub(crate) components: Vec<Box<dyn Component>>,
	pub(crate) parent_id: Option<NodeId>,
	pub(crate) child_ids: Vec<NodeId>,
	/// 지난번 그릴 때 변환의 가장 큰 배율. (0 이면 아직 모른다, 글 굽기 배율을 움직이는 동안 묶는 데 쓴다)
	last_drawn_maximum_scale: f64,
}

impl WorldNode {
	//==============================================================================
	/// 생성. (트리에 넣을 때는 NodeTree::create_node 를 쓴다)
	///
	/// # 인자
	/// - `name`: 노드 이름. (이름으로 찾기에 쓴다)
	///
	/// # 돌려줌
	/// 부모가 없고 활성인 새 노드. (터치는 받지 않는다)
	//==============================================================================
	pub fn new(name: &str) -> WorldNode {
		WorldNode {
			instance_id: identifier::create_instance_id(),
			name: name.to_string(),
			is_active: true,
			is_interactable: false,
			is_gizmo_visible: false,
			is_opacity_layer: false,
			transform: NodeTransform::new(),
			components: Vec::new(),
			parent_id: None,
			child_ids: Vec::new(),
			last_drawn_maximum_scale: 0.0,
		}
	}

	//==============================================================================
	/// 인스턴스 식별자.
	///
	/// # 돌려줌
	/// 만들 때 받은 번호. (1 부터, 노드마다 겹치지 않는다)
	//==============================================================================
	pub fn get_instance_id(&self) -> u64 {
		self.instance_id
	}

	//==============================================================================
	/// 이름 설정.
	///
	/// # 인자
	/// - `name`: 새 이름.
	//==============================================================================
	pub fn set_name(&mut self, name: &str) {
		self.name = name.to_string();
	}

	//==============================================================================
	/// 이름 반환.
	///
	/// # 돌려줌
	/// 노드 이름.
	//==============================================================================
	pub fn get_name(&self) -> &str {
		&self.name
	}

	//==============================================================================
	/// 부모 핸들. (루트이거나 아직 붙지 않았으면 None)
	///
	/// # 돌려줌
	/// 부모 핸들. 루트이거나 아직 붙지 않았으면 None.
	//==============================================================================
	pub fn get_parent_id(&self) -> Option<NodeId> {
		self.parent_id
	}

	//==============================================================================
	/// 자식 핸들 목록. (그리는 순서)
	///
	/// # 돌려줌
	/// 자식 핸들 목록. (앞이 먼저 그려진다)
	//==============================================================================
	pub fn get_child_ids(&self) -> &[NodeId] {
		&self.child_ids
	}

	//==============================================================================
	/// 자식 수.
	///
	/// # 돌려줌
	/// 바로 아래 자식 수.
	//==============================================================================
	pub fn get_child_count(&self) -> usize {
		self.child_ids.len()
	}

	//==============================================================================
	/// 부모가 없는지. (원본 isRoot, 트리의 루트와 아직 붙이지 않은 노드)
	///
	/// # 돌려줌
	/// 부모가 없으면 true.
	//==============================================================================
	pub fn is_root(&self) -> bool {
		self.parent_id.is_none()
	}

	//==============================================================================
	/// 자식이 없는지. (원본 isLeaf)
	///
	/// # 돌려줌
	/// 자식이 없으면 true.
	//==============================================================================
	pub fn is_leaf(&self) -> bool {
		self.child_ids.is_empty()
	}

	//==============================================================================
	/// 부모가 있는지. (원본 hasParent)
	///
	/// # 돌려줌
	/// 부모가 있으면 true.
	//==============================================================================
	pub fn has_parent(&self) -> bool {
		self.parent_id.is_some()
	}

	//==============================================================================
	/// 그 자리의 자식. (범위 밖이면 None, 원본 getChild 의 null)
	///
	/// # 인자
	/// - `child_index`: 자식 자리. (0 부터, 그리는 순서)
	///
	/// # 돌려줌
	/// 그 자리의 자식 핸들. 범위 밖이면 None.
	//==============================================================================
	pub fn get_child(&self, child_index: usize) -> Option<NodeId> {
		self.child_ids.get(child_index).copied()
	}

	//==============================================================================
	/// 그 노드가 자식인지. (원본 hasChild)
	///
	/// # 인자
	/// - `child_id`: 찾을 노드.
	///
	/// # 돌려줌
	/// 바로 아래 자식이면 true.
	//==============================================================================
	pub fn has_child(&self, child_id: NodeId) -> bool {
		self.child_ids.contains(&child_id)
	}

	//==============================================================================
	/// 자식의 자리. (그리는 순서의 번호, 자식이 아니면 None. 원본 getChildIndex 의 -1)
	///
	/// # 인자
	/// - `child_id`: 찾을 자식.
	///
	/// # 돌려줌
	/// 그리는 순서의 번호. (0 부터) 바로 아래 자식이 아니면 None.
	//==============================================================================
	pub fn get_child_index(&self, child_id: NodeId) -> Option<usize> {
		self.child_ids.iter().position(|candidate_id| *candidate_id == child_id)
	}

	//==============================================================================
	/// 활성화 설정. (꺼지면 tick 도 draw 도 하지 않는다)
	///
	/// # 인자
	/// - `is_active`: 켤지. (false 면 이 노드와 자손이 tick, 그리기, 터치에서 빠진다)
	//==============================================================================
	pub fn set_active(&mut self, is_active: bool) {
		self.is_active = is_active;
	}

	//==============================================================================
	/// 활성화 여부.
	///
	/// # 돌려줌
	/// 자기 활성 플래그. (조상까지 보려면 NodeTree::is_active_in_hierarchy)
	//==============================================================================
	pub fn is_active(&self) -> bool {
		self.is_active
	}

	//==============================================================================
	/// 터치를 받을지 설정.
	///
	/// # 인자
	/// - `is_interactable`: 터치를 받을지. (기본 false)
	//==============================================================================
	pub fn set_interactable(&mut self, is_interactable: bool) {
		self.is_interactable = is_interactable;
	}

	//==============================================================================
	/// 터치를 받는지.
	///
	/// # 돌려줌
	/// 터치를 받으면 true.
	//==============================================================================
	pub fn is_interactable(&self) -> bool {
		self.is_interactable
	}

	//==============================================================================
	/// 기즈모 표시 설정.
	///
	/// # 인자
	/// - `is_gizmo_visible`: 기즈모(내용 테두리와 피봇 점)를 그릴지.
	//==============================================================================
	pub fn set_gizmo_visible(&mut self, is_gizmo_visible: bool) {
		self.is_gizmo_visible = is_gizmo_visible;
	}

	//==============================================================================
	/// 기즈모 표시 여부.
	///
	/// # 돌려줌
	/// 기즈모를 그리면 true.
	//==============================================================================
	pub fn is_gizmo_visible(&self) -> bool {
		self.is_gizmo_visible
	}

	//==============================================================================
	/// 투명도 층 설정. (CSS opacity 처럼 투명도를 노드와 자손에 묶어 한 번에 얹을지)
	/// - 켜고 투명도(부모의 것을 곱한 값)가 1 보다 작으면 NodeTree 가 이 노드와 자손을 층에 모아 그렸다가 그 투명도로 한 번에 얹는다. (Graphic::begin_opacity_layer)
	///   겹친 자식끼리 비치지 않는다. 끄면(처음) 노드마다 투명도를 곱해 따로 얹는다.
	///
	/// # 인자
	/// - `is_opacity_layer`: 투명도를 묶어 얹을지.
	///
	/// # 예제
	/// 반투명한 묶음 노드 (겹친 자식이 서로 비치지 않는다)
	/// ```rust
	/// use vanilla::NodeTree;
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let group_id = node_tree.create_child(root_id, "group");
	/// node_tree[group_id].set_local_opacity(0.5);
	/// node_tree[group_id].set_opacity_layer(true);
	/// assert!(node_tree[group_id].is_opacity_layer());
	/// ```
	//==============================================================================
	pub fn set_opacity_layer(&mut self, is_opacity_layer: bool) {
		self.is_opacity_layer = is_opacity_layer;
	}

	//==============================================================================
	/// 투명도 층 여부.
	///
	/// # 돌려줌
	/// 투명도를 노드와 자손에 묶어 얹으면 true.
	//==============================================================================
	pub fn is_opacity_layer(&self) -> bool {
		self.is_opacity_layer
	}

	//==============================================================================
	/// 트랜스폼 반환.
	///
	/// # 돌려줌
	/// 노드 트랜스폼.
	//==============================================================================
	pub fn get_transform(&self) -> &NodeTransform {
		&self.transform
	}

	//==============================================================================
	/// 트랜스폼 반환. (변경용)
	///
	/// # 돌려줌
	/// 노드 트랜스폼. (변경용)
	//==============================================================================
	pub fn get_transform_mut(&mut self) -> &mut NodeTransform {
		&mut self.transform
	}

	//==============================================================================
	/// 트랜스폼 바로가기.
	/// - 로컬 위치 설정. (NodeTransform::set_local_position)
	///
	/// # 인자
	/// - `local_position`: 부모 내용 영역의 왼쪽 위에서 본 피봇 점의 자리.
	//==============================================================================
	pub fn set_local_position(&mut self, local_position: Vector2) {
		self.transform.set_local_position(local_position);
	}

	//==============================================================================
	/// 로컬 위치 반환. (NodeTransform::get_local_position)
	///
	/// # 돌려줌
	/// 로컬 위치. (부모 내용 영역의 왼쪽 위에서 본 피봇 점의 자리)
	//==============================================================================
	pub fn get_local_position(&self) -> Vector2 {
		self.transform.get_local_position()
	}

	//==============================================================================
	/// 로컬 배율 설정. (NodeTransform::set_local_scale)
	///
	/// # 인자
	/// - `local_scale`: 축마다 배율. (1 이 원래 크기, 음수면 그 축으로 뒤집혀 그려진다)
	//==============================================================================
	pub fn set_local_scale(&mut self, local_scale: Vector2) {
		self.transform.set_local_scale(local_scale);
	}

	//==============================================================================
	/// 로컬 배율 반환. (NodeTransform::get_local_scale)
	///
	/// # 돌려줌
	/// 로컬 배율.
	//==============================================================================
	pub fn get_local_scale(&self) -> Vector2 {
		self.transform.get_local_scale()
	}

	//==============================================================================
	/// 로컬 회전 설정. (NodeTransform::set_local_rotation)
	///
	/// # 인자
	/// - `local_rotation`: 회전. (도, 양수면 화면에서 시계 방향)
	//==============================================================================
	pub fn set_local_rotation(&mut self, local_rotation: f64) {
		self.transform.set_local_rotation(local_rotation);
	}

	//==============================================================================
	/// 로컬 회전 반환. (NodeTransform::get_local_rotation)
	///
	/// # 돌려줌
	/// 로컬 회전. (도)
	//==============================================================================
	pub fn get_local_rotation(&self) -> f64 {
		self.transform.get_local_rotation()
	}

	//==============================================================================
	/// 로컬 불투명도 설정. (NodeTransform::set_local_opacity, 자식에게 곱해져 내려간다)
	///
	/// # 인자
	/// - `local_opacity`: 불투명도. (0 ~ 1, 밖의 값은 잘라 넣는다)
	//==============================================================================
	pub fn set_local_opacity(&mut self, local_opacity: f64) {
		self.transform.set_local_opacity(local_opacity);
	}

	//==============================================================================
	/// 로컬 불투명도 반환. (NodeTransform::get_local_opacity)
	///
	/// # 돌려줌
	/// 로컬 불투명도. (0 ~ 1)
	//==============================================================================
	pub fn get_local_opacity(&self) -> f64 {
		self.transform.get_local_opacity()
	}

	//==============================================================================
	/// 피봇 설정. (NodeTransform::set_pivot)
	///
	/// # 인자
	/// - `pivot`: 내용 크기 안의 기준점 비율. ((0, 0) 왼쪽 위, (1, 1) 오른쪽 아래, Pivot 상수)
	//==============================================================================
	pub fn set_pivot(&mut self, pivot: Vector2) {
		self.transform.set_pivot(pivot);
	}

	//==============================================================================
	/// 피봇 반환. (NodeTransform::get_pivot)
	///
	/// # 돌려줌
	/// 피봇. (내용 크기 안의 비율)
	//==============================================================================
	pub fn get_pivot(&self) -> Vector2 {
		self.transform.get_pivot()
	}

	//==============================================================================
	/// 내용 크기 설정. (NodeTransform::set_content_size)
	///
	/// # 인자
	/// - `content_size`: 내용 크기. (노드 좌표, 배율을 걸기 전)
	//==============================================================================
	pub fn set_content_size(&mut self, content_size: Vector2) {
		self.transform.set_content_size(content_size);
	}

	//==============================================================================
	/// 내용 크기 반환. (NodeTransform::get_content_size)
	///
	/// # 돌려줌
	/// 내용 크기. (노드 좌표, 배율을 걸기 전)
	//==============================================================================
	pub fn get_content_size(&self) -> Vector2 {
		self.transform.get_content_size()
	}

	//==============================================================================
	/// 로컬 위치 설정. (x, y 를 바로 받는다, NodeTransform::set_local_position_xy)
	///
	/// # 인자
	/// - `x`: 로컬 위치 x.
	/// - `y`: 로컬 위치 y.
	//==============================================================================
	pub fn set_local_position_xy(&mut self, x: f64, y: f64) {
		self.transform.set_local_position_xy(x, y);
	}

	//==============================================================================
	/// 로컬 배율 설정. (x, y 를 바로 받는다, NodeTransform::set_local_scale_xy)
	///
	/// # 인자
	/// - `x`: 가로 배율.
	/// - `y`: 세로 배율.
	//==============================================================================
	pub fn set_local_scale_xy(&mut self, x: f64, y: f64) {
		self.transform.set_local_scale_xy(x, y);
	}

	//==============================================================================
	/// 내용 크기 설정. (너비, 높이를 바로 받는다, NodeTransform::set_content_size_xy)
	///
	/// # 인자
	/// - `width`: 내용 너비.
	/// - `height`: 내용 높이.
	//==============================================================================
	pub fn set_content_size_xy(&mut self, width: f64, height: f64) {
		self.transform.set_content_size_xy(width, height);
	}

	//==============================================================================
	/// 부모 영역 안의 사각형에 놓기. (NodeTransform::set_rect, 피봇이 어디든 왼쪽 위가 (x, y) 에 온다)
	///
	/// # 인자
	/// - `x`: 왼쪽 위 x. (부모 내용 영역의 왼쪽 위 기준)
	/// - `y`: 왼쪽 위 y. (부모 내용 영역의 왼쪽 위 기준)
	/// - `width`: 내용 너비.
	/// - `height`: 내용 높이.
	//==============================================================================
	pub fn set_rect(&mut self, x: f64, y: f64, width: f64, height: f64) {
		self.transform.set_rect(x, y, width, height);
	}

	//==============================================================================
	/// 지난번 그릴 때 변환의 가장 큰 배율. (NodeTree 가 그릴 때 Graphic.update_text_bake_scale_moving 으로 쓴다)
	///
	/// # 인자
	/// - `last_drawn_maximum_scale`: 지난번 그릴 때 변환의 가장 큰 배율. (0 이면 모른다)
	//==============================================================================
	pub fn set_last_drawn_maximum_scale(&mut self, last_drawn_maximum_scale: f64) {
		self.last_drawn_maximum_scale = last_drawn_maximum_scale;
	}

	//==============================================================================
	/// 지난번 그릴 때 변환의 가장 큰 배율 반환.
	///
	/// # 돌려줌
	/// 가장 큰 배율. (0 이면 아직 모른다)
	//==============================================================================
	pub fn get_last_drawn_maximum_scale(&self) -> f64 {
		self.last_drawn_maximum_scale
	}

	//==============================================================================
	/// 앵커 설정. (NodeTransform::set_anchor)
	///
	/// # 인자
	/// - `anchor`: 부모 내용 크기 안의 비율. (anchored_position 의 기준점, Pivot 상수)
	//==============================================================================
	pub fn set_anchor(&mut self, anchor: Vector2) {
		self.transform.set_anchor(anchor);
	}

	//==============================================================================
	/// 앵커 반환. (NodeTransform::get_anchor)
	///
	/// # 돌려줌
	/// 앵커. (부모 내용 크기 안의 비율)
	//==============================================================================
	pub fn get_anchor(&self) -> Vector2 {
		self.transform.get_anchor()
	}

	//==============================================================================
	/// 보이기 설정. (NodeTransform::set_visible, 꺼도 tick 은 돈다)
	///
	/// # 인자
	/// - `is_visible`: 그릴지. (false 면 이 노드와 자손을 그리지 않고 터치도 받지 않는다)
	//==============================================================================
	pub fn set_visible(&mut self, is_visible: bool) {
		self.transform.set_visible(is_visible);
	}

	//==============================================================================
	/// 보이기 여부. (NodeTransform::is_visible)
	///
	/// # 돌려줌
	/// 보이면 true.
	//==============================================================================
	pub fn is_visible(&self) -> bool {
		self.transform.is_visible()
	}

	//==============================================================================
	/// 컴포넌트 추가. (추가한 컴포넌트를 돌려준다)
	/// - 붙이기 전에 의존 컴포넌트(Component::require)가 없으면 만들어 붙이고, 그 뒤 attach 를 부른다.
	/// - attach_to_tree 는 부르지 않는다. 붙을 때 자식 노드를 만드는 컴포넌트는 NodeTree::add_component 로 붙인다.
	///
	/// # 인자
	/// - `component`: 붙일 컴포넌트.
	///
	/// # 돌려줌
	/// 붙인 컴포넌트. (변경용)
	///
	/// # 예제
	/// 붙이면서 돌려받은 컴포넌트를 바로 고치기
	/// ```rust
	/// use vanilla::{Color, Paint, WorldNode};
	///
	/// let mut node = WorldNode::new("panel");
	/// let paint = node.add_component(Paint::new());
	/// paint.set_color(Color::create(1.0, 0.0, 0.0, 1.0));
	/// assert_eq!(node.get_component::<Paint>().expect("paint").get_color(), Color::create(1.0, 0.0, 0.0, 1.0));
	/// ```
	//==============================================================================
	pub fn add_component<T: Component + 'static>(&mut self, component: T) -> &mut T {
		// 원본 addComponent 처럼 자기 자리를 먼저 잡고, 의존 컴포넌트를 그 뒤에 붙인 다음 붙는다.
		let component_index = self.components.len();
		let boxed_component: Box<dyn Component> = Box::new(component);
		let required_component_list = boxed_component.require();
		self.components.push(boxed_component);
		self.add_required_components(required_component_list);
		self.components[component_index].attach(&mut self.transform);
		let component_reference: &mut dyn Component = self.components[component_index].as_mut();
		component_reference.as_any_mut().downcast_mut::<T>().expect("added component type")
	}

	//==============================================================================
	/// 의존 컴포넌트 붙이기. (그 타입이 없을 때만, 의존 컴포넌트의 의존도 먼저 붙인다)
	//==============================================================================
	fn add_required_components(&mut self, required_component_list: Vec<RequiredComponent>) {
		for required_component in required_component_list {
			let is_present = self.components.iter().any(|component| {
				let component_reference: &dyn Component = component.as_ref();
				Any::type_id(component_reference.as_any()) == required_component.type_id
			});
			if is_present {
				continue;
			}
			let mut boxed_component = (required_component.create)();
			self.add_required_components(boxed_component.require());
			boxed_component.attach(&mut self.transform);
			self.components.push(boxed_component);
		}
	}

	//==============================================================================
	/// 컴포넌트 반환. 없으면 기본값으로 만들어 붙이고 반환. (원본 getOrAddComponent)
	///
	/// # 돌려줌
	/// 그 타입의 첫 컴포넌트. (변경용) 없었으면 새로 붙인 기본값.
	///
	/// # 예제
	/// 두 번 불러도 하나만 붙는다
	/// ```rust
	/// use vanilla::{Paint, WorldNode};
	///
	/// let mut node = WorldNode::new("node");
	/// node.get_or_add_component::<Paint>().set_round_size(8.0);
	/// assert_eq!(node.get_or_add_component::<Paint>().get_round_size(), 8.0);
	/// assert_eq!(node.get_components::<Paint>().len(), 1);
	/// ```
	//==============================================================================
	pub fn get_or_add_component<T: Component + Default + 'static>(&mut self) -> &mut T {
		if !self.has_component::<T>() {
			return self.add_component(T::default());
		}
		self.get_component_mut::<T>().expect("present component")
	}

	//==============================================================================
	/// 그 타입의 컴포넌트 모두. (원본 getComponents)
	///
	/// # 돌려줌
	/// 그 타입의 컴포넌트 모두. (붙인 차례) 없으면 빈 목록.
	//==============================================================================
	pub fn get_components<T: Component + 'static>(&self) -> Vec<&T> {
		self.components.iter().filter_map(|component| {
			let component_reference: &dyn Component = component.as_ref();
			component_reference.as_instance_of::<T>()
		}).collect()
	}

	//==============================================================================
	/// 컴포넌트 반환. (그 타입의 첫 번째)
	///
	/// # 돌려줌
	/// 그 타입의 첫 컴포넌트. 없으면 None.
	//==============================================================================
	pub fn get_component<T: Component + 'static>(&self) -> Option<&T> {
		for component in self.components.iter() {
			let component_reference: &dyn Component = component.as_ref();
			if let Some(found) = component_reference.as_instance_of::<T>() {
				return Some(found);
			}
		}
		None
	}

	//==============================================================================
	/// 컴포넌트 반환. (변경용, 그 타입의 첫 번째)
	///
	/// # 돌려줌
	/// 그 타입의 첫 컴포넌트. (변경용) 없으면 None.
	//==============================================================================
	pub fn get_component_mut<T: Component + 'static>(&mut self) -> Option<&mut T> {
		for component in self.components.iter_mut() {
			let component_reference: &mut dyn Component = component.as_mut();
			if let Some(found) = component_reference.as_instance_of_mut::<T>() {
				return Some(found);
			}
		}
		None
	}

	//==============================================================================
	/// 컴포넌트가 있는지.
	///
	/// # 돌려줌
	/// 그 타입의 컴포넌트가 있으면 true.
	//==============================================================================
	pub fn has_component<T: Component + 'static>(&self) -> bool {
		self.get_component::<T>().is_some()
	}

	//==============================================================================
	/// 컴포넌트 제거. (그 타입 모두, 제거한 수)
	/// - 떼는 컴포넌트마다 detach 를 부른다.
	///
	/// # 돌려줌
	/// 뗀 컴포넌트 수. (없으면 0)
	///
	/// # 예제
	/// 그 타입을 모두 떼기
	/// ```rust
	/// use vanilla::{Paint, WorldNode};
	///
	/// let mut node = WorldNode::new("node");
	/// node.add_component(Paint::new());
	/// node.add_component(Paint::new());
	/// assert_eq!(node.remove_component::<Paint>(), 2);
	/// assert!(!node.has_component::<Paint>());
	/// assert_eq!(node.remove_component::<Paint>(), 0);
	/// ```
	//==============================================================================
	pub fn remove_component<T: Component + 'static>(&mut self) -> usize {
		let before_count = self.components.len();
		let transform = &mut self.transform;
		self.components.retain_mut(|component| {
			let component_reference: &dyn Component = component.as_ref();
			if !component_reference.is_instance_of::<T>() {
				return true;
			}
			component.detach(transform);
			false
		});
		before_count - self.components.len()
	}

	//==============================================================================
	/// 모든 컴포넌트.
	///
	/// # 돌려줌
	/// 붙인 컴포넌트 모두. (붙인 차례)
	//==============================================================================
	pub fn get_all_components(&self) -> &Vec<Box<dyn Component>> {
		&self.components
	}

	//==============================================================================
	/// 출력 상태 시작. (이동 → 회전 → 크기 → 피봇 보정 → 투명도 곱)
	/// - Graphic 상태를 하나 쌓은 뒤 건다. pop_transform 과 짝으로 부른다.
	///
	/// # 인자
	/// - `graphic`: 트랜스폼을 걸 Graphic.
	//==============================================================================
	pub fn push_transform(&self, graphic: &mut Graphic) {
		graphic.push_state();

		// 트랜스폼 반영.
		// local_position 은 부모 영역 내 자기 객체의 기준점 좌표 (anchor 영향 없음).
		let local_position = self.transform.get_local_position();
		let local_rotation = self.transform.get_local_rotation();
		let radian = math::degree_to_radian(local_rotation);
		let local_scale = self.transform.get_local_scale();
		graphic.translate(local_position.x, local_position.y);
		graphic.rotate(radian);
		graphic.scale(local_scale.x, local_scale.y);

		// 현재 노드의 피봇 반영.
		// 기준점을 좌상으로 여기고 우하방향으로 그림을 그리므로, 컨텐트사이즈 기준 피봇만큼 당긴다.
		let pivot = self.transform.get_pivot();
		let content_size = self.transform.get_content_size();
		graphic.translate(-content_size.x * pivot.x, -content_size.y * pivot.y);

		// 투명도 반영.
		let local_opacity = self.transform.get_local_opacity();
		graphic.multiply_global_alpha(local_opacity);
	}

	//==============================================================================
	/// 컴포넌트의 노드 출력 상태 시작. (push_transform 바로 뒤, 켜진 컴포넌트를 붙인 차례로)
	///
	/// # 인자
	/// - `graphic`: 그리는 Graphic. (push_transform 이 걸린 상태)
	//==============================================================================
	pub fn push_component_transforms(&mut self, graphic: &mut Graphic) {
		for component in self.components.iter_mut() {
			if component.is_enable() {
				component.push_transform(&self.transform, graphic);
			}
		}
	}

	//==============================================================================
	/// 컴포넌트의 노드 출력 상태 끝. (pop_transform 바로 앞, 켜진 컴포넌트를 붙인 차례의 거꾸로)
	///
	/// # 인자
	/// - `graphic`: 그리는 Graphic.
	//==============================================================================
	pub fn pop_component_transforms(&mut self, graphic: &mut Graphic) {
		for component in self.components.iter_mut().rev() {
			if component.is_enable() {
				component.pop_transform(&self.transform, graphic);
			}
		}
	}

	//==============================================================================
	/// 컴포넌트 출력. (자식은 NodeTree 가 그린다)
	/// - 돌려주는 것: (Mask 가 있으면 둥근 크기, 화면 밖이라 건너뛰었는지)
	/// - 켜진 컴포넌트가 모두 그리는 사각형을 알려 주면(calculate_drawn_rect) 그 합을 지금 Graphic 변환으로 화면에 대 보고, 밖이면 그리지 않는다.
	///   하나라도 모르면 거르지 않는다. 걸러도 자식은 NodeTree 가 따로 판정한다.
	///
	/// # 인자
	/// - `graphic`: 그릴 Graphic. (push_transform 이 걸린 상태)
	///
	/// # 돌려줌
	/// (켜진 자르는 컴포넌트(Mask)의 귀퉁이 둥근 크기, 없으면 None. 화면 밖이라 그리지 않았으면 true)
	//==============================================================================
	pub fn draw_components(&mut self, graphic: &mut Graphic) -> (Option<f64>, bool) {
		let mut is_culled = false;
		if !self.components.is_empty() && graphic.is_culling_enabled() {
			let mut drawn_rect: Option<Rect> = None;
			let mut is_drawn_rect_known = true;
			for component in self.components.iter_mut() {
				if !component.is_enable() {
					continue;
				}
				let component_rect = match component.calculate_drawn_rect(&self.transform, graphic) {
					Some(component_rect) => {
						component_rect
					}
					None => {
						is_drawn_rect_known = false;
						break;
					}
				};
				drawn_rect = match drawn_rect {
					Some(drawn_rect) => {
						let left_x = drawn_rect.position.x.min(component_rect.position.x);
						let top_y = drawn_rect.position.y.min(component_rect.position.y);
						let right_x = (drawn_rect.position.x + drawn_rect.size.x).max(component_rect.position.x + component_rect.size.x);
						let bottom_y = (drawn_rect.position.y + drawn_rect.size.y).max(component_rect.position.y + component_rect.size.y);
						Some(Rect::create(left_x, top_y, right_x - left_x, bottom_y - top_y))
					}
					None => {
						Some(component_rect)
					}
				};
			}
			if is_drawn_rect_known {
				if let Some(drawn_rect) = drawn_rect {
					is_culled = !graphic.is_local_rect_visible(drawn_rect);
				}
			}
		}

		let mut clip_round_size: Option<f64> = None;
		for component in self.components.iter_mut() {
			if !component.is_enable() {
				continue;
			}
			if !is_culled {
				component.draw(&self.transform, graphic);
			}
			if let Some(round_size) = component.get_clip_round_size() {
				clip_round_size = Some(round_size);
			}
		}
		(clip_round_size, is_culled)
	}

	//==============================================================================
	/// 기즈모 출력. (content_size 테두리와 피봇 점)
	/// - 전역 알파를 잠깐 1 로 두고 그린 뒤 되돌린다. 기즈모 표시가 켜진 컴포넌트의 draw_gizmos 도 부른다.
	///
	/// # 인자
	/// - `graphic`: 그릴 Graphic. (push_transform 이 걸린 상태)
	//==============================================================================
	pub fn draw_gizmos(&mut self, graphic: &mut Graphic) {
		let original_alpha = graphic.get_global_alpha();
		graphic.set_global_alpha(1.0);

		let content_size = self.transform.get_content_size();
		let pivot = self.transform.get_pivot();
		graphic.set_stroke_color(Color::create(0.2, 1.0, 0.4, 1.0));
		graphic.draw_stroke_rect(Rect::create(0.0, 0.0, content_size.x, content_size.y), 1.0);
		graphic.set_fill_color(Color::create(1.0, 0.3, 0.3, 1.0));
		graphic.draw_circle(content_size.multiply(pivot), 3.0);

		for component in self.components.iter_mut() {
			if component.is_gizmo_visible() {
				component.draw_gizmos(&self.transform, graphic);
			}
		}

		graphic.set_global_alpha(original_alpha);
	}

	//==============================================================================
	/// 출력 상태 종료.
	///
	/// # 인자
	/// - `graphic`: push_transform 에서 쌓은 상태를 꺼낼 Graphic.
	//==============================================================================
	pub fn pop_transform(&self, graphic: &mut Graphic) {
		graphic.pop_state();
	}

	//==============================================================================
	/// 점이 이 노드의 content_size 영역 안에 있는지. (world_matrix 는 이 노드까지 누적한 행렬)
	///
	/// # 인자
	/// - `world_matrix`: 루트부터 이 노드까지 곱한 행렬. (NodeTree::calculate_world_matrix)
	/// - `view_position`: 볼 점. (뷰 좌표)
	///
	/// # 돌려줌
	/// 점이 내용 영역 (0, 0) ~ content_size 안이면 true. (경계 포함) 행렬을 뒤집을 수 없으면 false.
	///
	/// # 예제
	/// 왼쪽 위 피봇 노드의 영역 안과 밖
	/// ```rust
	/// use vanilla::{NodeTree, Pivot, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let node_id = node_tree.create_child(root_id, "button");
	/// node_tree[node_id].set_pivot(Pivot::TOP_LEFT);
	/// node_tree[node_id].set_rect(100.0, 100.0, 50.0, 30.0);
	/// let world_matrix = node_tree.calculate_world_matrix(node_id);
	/// assert!(node_tree[node_id].contains_point(&world_matrix, Vector2::create(120.0, 110.0)));
	/// assert!(!node_tree[node_id].contains_point(&world_matrix, Vector2::create(10.0, 10.0)));
	/// ```
	//==============================================================================
	pub fn contains_point(&self, world_matrix: &TransformMatrix, view_position: Vector2) -> bool {
		let inverse_matrix = match world_matrix.invert() {
			Some(inverse_matrix) => {
				inverse_matrix
			}
			None => {
				return false;
			}
		};
		let local_point = inverse_matrix.transform_point(view_position.x, view_position.y);
		let content_size = self.transform.get_content_size();
		local_point.x >= 0.0 && local_point.y >= 0.0 && local_point.x <= content_size.x && local_point.y <= content_size.y
	}
}
