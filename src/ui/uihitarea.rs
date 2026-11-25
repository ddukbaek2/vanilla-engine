//==============================================================================
// 누르는 자리 넓히기. (vanilla.js ui/uihitarea.js 의 UIHitArea)
// - 보이는 단추는 그대로 두고, 단추 바로 뒤 형제로 투명한 누름 자리를 두어 가로세로를 가장 작은 크기 이상으로 넓힌다. (손가락으로 누르기 좋게)
// - 그리는 차례가 늦은 노드가 누름을 받으므로, 단추 위는 단추가 받고 단추 둘레의 넓힌 몫만 누름 자리가 받는다.
// - 누름 자리를 누르면 단추의 클릭 알림(Clicked, Click)을 그대로 부른다. 단추가 꺼지거나 누를 수 없으면 누름 자리도 따른다.
// - 단추의 자리, 크기, 피봇을 프레임마다 따라가므로 화면이 단추를 다시 놓아도 맞는다.
// - 자르는 노드를 정하면 넓힌 몫이 그 노드의 사각형 밖으로 나가지 않게 자른다. (단추 자신의 자리는 그대로, 창 밖이나 스크롤 칸 밖이 눌리지 않게)
// - 사용: UIHitArea::attach(node_tree, button_id, minimum_size) (단추가 부모에 붙은 뒤에 부른다)
//==============================================================================
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::ui::uibutton::UIButton;

pub struct UIHitArea {
	component_base: ComponentBase,
	target_id: Option<NodeId>,
	minimum_size: f32,
	clip_id: Option<NodeId>,
}

#[crate::reflect]
impl UIHitArea {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIHitArea {
		UIHitArea {
			component_base: ComponentBase::new(),
			target_id: None,
			minimum_size: 0.0,
			clip_id: None,
		}
	}

	//==============================================================================
	// 단추에 넓힌 누름 자리 달기. (단추의 바로 뒤 형제로 끼우고 누름 자리 노드를 돌려준다, 부모가 없거나 UIButton 이 없으면 None)
	//==============================================================================
	pub fn attach(node_tree: &mut NodeTree, button_id: NodeId, minimum_size: f32) -> Option<NodeId> {
		let parent_id = node_tree.get_parent_id(button_id)?;
		node_tree.get_node(button_id)?.get_component::<UIButton>()?;
		let button_index = node_tree.get_child_ids(parent_id).iter().position(|child_id| *child_id == button_id)?;
		let button_name = String::from(node_tree.get_node(button_id)?.get_name());
		let hit_id = node_tree.create_node(&format!("{}HitArea", button_name));
		node_tree.insert_child(parent_id, hit_id, button_index);
		node_tree[hit_id].set_pivot(Pivot::TOP_LEFT);
		let hit_button = node_tree.add_component(hit_id, UIButton::new())?;
		// 넓힌 자리의 클릭을 단추가 받은 것처럼 부른다.
		hit_button.set_click_event(Box::new(move |_hit_button: &mut UIButton, context: &mut ComponentContext| {
			context.with_component_mut::<UIButton, ()>(button_id, |button, button_context| {
				button.fire_click_events(button_context);
			});
		}));
		let hit_area = node_tree.add_component(hit_id, UIHitArea::new())?;
		hit_area.set_target(button_id);
		hit_area.set_minimum_size(minimum_size);
		Some(hit_id)
	}

	//==============================================================================
	// 넓혀 줄 단추의 노드 설정.
	//==============================================================================
	pub fn set_target(&mut self, target_id: NodeId) {
		self.target_id = Some(target_id);
	}

	//==============================================================================
	// 넓혀 줄 단추의 노드 반환.
	//==============================================================================
	#[reflect(name = "TargetNode")]
	pub fn get_target(&self) -> Option<NodeId> {
		self.target_id
	}

	//==============================================================================
	// 넓혀 줄 단추. (원본 getTargetButton, 원본은 단추 컴포넌트를 쥐고 러스트 판은 대상 노드에서 꺼내므로 트리를 받는다)
	//==============================================================================
	pub fn get_target_button<'a>(&self, node_tree: &'a NodeTree) -> Option<&'a UIButton> {
		node_tree.get_node(self.target_id?)?.get_component::<UIButton>()
	}

	//==============================================================================
	// 누르는 자리의 가장 작은 가로세로 설정.
	//==============================================================================
	pub fn set_minimum_size(&mut self, minimum_size: f32) {
		self.minimum_size = minimum_size;
	}

	//==============================================================================
	// 누르는 자리의 가장 작은 가로세로 반환.
	//==============================================================================
	pub fn get_minimum_size(&self) -> f32 {
		self.minimum_size
	}

	//==============================================================================
	// 넓힌 몫을 자르는 노드 설정. (None 이면 자르지 않는다)
	//==============================================================================
	pub fn set_clip_node(&mut self, clip_id: Option<NodeId>) {
		self.clip_id = clip_id;
	}

	//==============================================================================
	// 넓힌 몫을 자르는 노드 반환.
	//==============================================================================
	pub fn get_clip_node(&self) -> Option<NodeId> {
		self.clip_id
	}

	//==============================================================================
	// 자르는 노드의 사각형을 부모 안의 좌표로. (왼쪽, 위, 오른쪽, 아래)
	//==============================================================================
	fn read_clip_rect(node_tree: &NodeTree, clip_id: NodeId, parent_id: NodeId) -> Option<[f32; 4]> {
		let clip_size = node_tree.get_node(clip_id)?.get_content_size();
		let corners = [Vector2::zero(), Vector2::create(clip_size.x, 0.0), Vector2::create(0.0, clip_size.y), clip_size];
		let mut minimum = Vector2::create(f32::MAX, f32::MAX);
		let mut maximum = Vector2::create(f32::MIN, f32::MIN);
		for corner in corners {
			let view_position = node_tree.convert_node_position_to_view_position(clip_id, corner);
			let parent_position = node_tree.convert_view_position_to_node_position(parent_id, view_position)?;
			minimum = Vector2::create(minimum.x.min(parent_position.x), minimum.y.min(parent_position.y));
			maximum = Vector2::create(maximum.x.max(parent_position.x), maximum.y.max(parent_position.y));
		}
		Some([minimum.x, minimum.y, maximum.x, maximum.y])
	}
}

impl Default for UIHitArea {
	fn default() -> UIHitArea {
		UIHitArea::new()
	}
}

#[crate::reflect]
impl Component for UIHitArea {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UIHitArea"
	}

	//==============================================================================
	// 단추 따라가기. (가운데를 맞추고 모자란 쪽만 넓힌다, 단추가 꺼져 있으면 함께 끈다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let target_id = match self.target_id {
			Some(target_id) => {
				target_id
			}
			None => {
				return;
			}
		};
		let node_tree = context.get_node_tree();
		let target_node = match node_tree.get_node(target_id) {
			Some(target_node) => {
				target_node
			}
			None => {
				context.get_node_mut().set_interactable(false);
				return;
			}
		};
		let is_target_interactable = target_node.get_component::<UIButton>().map(|button| button.get_interactable()).unwrap_or(false);
		let is_usable = target_node.is_active() && is_target_interactable;
		let target_position = target_node.get_local_position();
		let target_size = target_node.get_content_size();
		let target_pivot = target_node.get_pivot();
		let target_left = target_position.x - target_pivot.x * target_size.x;
		let target_top = target_position.y - target_pivot.y * target_size.y;
		let width = target_size.x.max(self.minimum_size);
		let height = target_size.y.max(self.minimum_size);
		let mut left = target_left - (width - target_size.x) * 0.5;
		let mut top = target_top - (height - target_size.y) * 0.5;
		let mut right = left + width;
		let mut bottom = top + height;
		let parent_id = node_tree.get_parent_id(context.get_node_id());
		if let (Some(clip_id), Some(parent_id)) = (self.clip_id, parent_id) {
			if let Some([clip_left, clip_top, clip_right, clip_bottom]) = UIHitArea::read_clip_rect(node_tree, clip_id, parent_id) {
				left = left.max(clip_left.min(target_left));
				top = top.max(clip_top.min(target_top));
				right = right.min(clip_right.max(target_left + target_size.x));
				bottom = bottom.min(clip_bottom.max(target_top + target_size.y));
			}
		}
		let node = context.get_node_mut();
		node.set_interactable(is_usable);
		node.set_local_position(Vector2::create(left, top));
		node.set_content_size(Vector2::create((right - left).max(0.0), (bottom - top).max(0.0)));
	}
}
