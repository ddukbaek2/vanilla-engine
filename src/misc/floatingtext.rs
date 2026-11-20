//==============================================================================
// 떠오르는 텍스트 관리 컴포넌트. (데미지 숫자 / +코인 등)
// - 원본 misc/floatingtext.js 를 옮긴 것이다. spawn 으로 띄우면 위로 떠오르며 사라지고, 끝난 라벨 노드는 다시 쓴다.
// - 씬 뿌리 밑에 노드 하나를 만들어 두고 월드 좌표로 띄운다.
// - 사용:
//     let floating_text_id = FloatingText::create(node_tree, root_id);
//     FloatingText::spawn(node_tree, floating_text_id, "-120", position, FloatingTextOptions { color: Color::create(1.0, 0.4, 0.4, 1.0), ..FloatingTextOptions::default() });
// - 원본과 다른 점:
//   원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드에 붙는 컴포넌트이고, create 가 노드를 만들어 붙여 준다.
//   spawn / clear 는 라벨 노드를 만들고 바꿔야 해서 트리와 노드 핸들을 받는 연관 함수다. (컴포넌트가 없으면 아무것도 하지 않는다)
//   라벨은 UILabel 대신 Text 컴포넌트로 그린다. (설정은 같다: 글, 크기, 색, 굵게)
//   options 객체는 FloatingTextOptions 구조체다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::text::Text;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};

//==============================================================================
// 띄우기 설정. (원본 options = { color?, font_size = 22, rise = 46, duration = 0.8, jitter = 10, bold = true })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FloatingTextOptions {
	// 글자 색. (기본 흰색)
	pub color: Color,
	pub font_size: f32,
	// 떠오르는 높이. (픽셀)
	pub rise: f32,
	// 사라질 때까지 걸리는 시간. (초)
	pub duration: f32,
	// 가로로 흩뿌리는 폭. (±픽셀)
	pub jitter: f32,
	pub is_bold: bool,
}

impl Default for FloatingTextOptions {
	//==============================================================================
	// 기본값. (흰색, 22, 46 픽셀, 0.8 초, ±10, 굵게)
	//==============================================================================
	fn default() -> FloatingTextOptions {
		FloatingTextOptions {
			color: Color::white(),
			font_size: 22.0,
			rise: 46.0,
			duration: 0.8,
			jitter: 10.0,
			is_bold: true,
		}
	}
}

//==============================================================================
// 떠 있는 항목 하나.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct FloatingTextItem {
	node_id: NodeId,
	start_position: Vector2,
	rise: f32,
	duration: f32,
	elapsed_seconds: f32,
}

//==============================================================================
// 떠오르는 텍스트 관리 컴포넌트.
//==============================================================================
pub struct FloatingText {
	component_base: ComponentBase,
	active_list: Vec<FloatingTextItem>,
	free_list: Vec<NodeId>,
}

impl FloatingText {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> FloatingText {
		FloatingText {
			component_base: ComponentBase::new(),
			active_list: Vec::new(),
			free_list: Vec::new(),
		}
	}

	//==============================================================================
	// 노드 만들기. (parent_id 밑에 "FloatingText" 노드를 만들고 컴포넌트를 붙인다. 원본 new FloatingText() + add_child)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId) -> NodeId {
		let floating_text_node_id = node_tree.create_child(parent_id, "FloatingText");
		let floating_text_node = &mut node_tree[floating_text_node_id];
		floating_text_node.set_pivot(Pivot::TOP_LEFT);
		floating_text_node.set_anchor(Pivot::TOP_LEFT);
		node_tree.add_component(floating_text_node_id, FloatingText::new());
		floating_text_node_id
	}

	//==============================================================================
	// 텍스트 띄우기. (쉬는 라벨 노드가 있으면 다시 쓰고, 없으면 만든다. 띄운 라벨 노드를 돌려준다)
	//==============================================================================
	pub fn spawn(node_tree: &mut NodeTree, floating_text_node_id: NodeId, text: &str, world_position: Vector2, options: FloatingTextOptions) -> Option<NodeId> {
		node_tree.get_node(floating_text_node_id)?.get_component::<FloatingText>()?;

		// 쉬는 라벨 노드 꺼내기. (밖에서 지웠거나 옮긴 노드는 버린다)
		let mut label_node_id: Option<NodeId> = None;
		loop {
			let candidate_id = match node_tree.get_node_mut(floating_text_node_id).and_then(|node| node.get_component_mut::<FloatingText>()) {
				Some(floating_text) => {
					floating_text.free_list.pop()
				}
				None => {
					None
				}
			};
			let candidate_id = match candidate_id {
				Some(candidate_id) => {
					candidate_id
				}
				None => {
					break;
				}
			};
			if node_tree.get_parent_id(candidate_id) == Some(floating_text_node_id) {
				label_node_id = Some(candidate_id);
				break;
			}
		}
		let label_node_id = match label_node_id {
			Some(label_node_id) => {
				label_node_id
			}
			None => {
				let created_node_id = node_tree.create_child(floating_text_node_id, "FloatingTextItem");
				let created_node = &mut node_tree[created_node_id];
				created_node.set_pivot(Pivot::MIDDLE_CENTER);
				created_node.set_anchor(Pivot::TOP_LEFT);
				created_node.set_content_size(Vector2::create(240.0, 40.0));
				node_tree.add_component(created_node_id, Text::new());
				created_node_id
			}
		};

		let jitter_x = math::random_range(-options.jitter, options.jitter);
		let start_position = Vector2::create(world_position.x + jitter_x, world_position.y);
		let label_node = &mut node_tree[label_node_id];
		label_node.set_active(true);
		label_node.set_local_opacity(1.0);
		label_node.set_local_position(start_position);
		if let Some(label_text) = label_node.get_component_mut::<Text>() {
			label_text.set_text(text);
			label_text.set_font_size(options.font_size);
			label_text.set_text_color(options.color);
			label_text.set_bold(options.is_bold);
		}

		let floating_text = node_tree.get_node_mut(floating_text_node_id)?.get_component_mut::<FloatingText>()?;
		floating_text.active_list.push(FloatingTextItem {
			node_id: label_node_id,
			start_position,
			rise: options.rise,
			duration: options.duration,
			elapsed_seconds: 0.0,
		});
		Some(label_node_id)
	}

	//==============================================================================
	// 떠 있는 항목 수 반환.
	//==============================================================================
	pub fn get_active_count(&self) -> usize {
		self.active_list.len()
	}

	//==============================================================================
	// 모두 지우기. (떠 있는 라벨을 끄고 쉬는 목록으로 돌린다)
	//==============================================================================
	pub fn clear(node_tree: &mut NodeTree, floating_text_node_id: NodeId) {
		let active_list = match node_tree.get_node_mut(floating_text_node_id).and_then(|node| node.get_component_mut::<FloatingText>()) {
			Some(floating_text) => {
				std::mem::take(&mut floating_text.active_list)
			}
			None => {
				return;
			}
		};
		let mut released_node_ids: Vec<NodeId> = Vec::new();
		for item in active_list {
			if let Some(label_node) = node_tree.get_node_mut(item.node_id) {
				label_node.set_active(false);
				released_node_ids.push(item.node_id);
			}
		}
		if let Some(floating_text) = node_tree.get_node_mut(floating_text_node_id).and_then(|node| node.get_component_mut::<FloatingText>()) {
			floating_text.free_list.extend(released_node_ids);
		}
	}
}

impl Default for FloatingText {
	//==============================================================================
	// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> FloatingText {
		FloatingText::new()
	}
}

impl Component for FloatingText {
	//==============================================================================
	// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"FloatingText"
	}

	//==============================================================================
	// 갱신. (떠오름, 사라짐, 다 끝난 라벨 회수. 뒤에서부터 돈다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		let node_tree = context.get_node_tree_mut();
		let mut item_index = self.active_list.len();
		while item_index > 0 {
			item_index -= 1;
			let item = &mut self.active_list[item_index];
			item.elapsed_seconds += time_delta;
			let linear_ratio = (item.elapsed_seconds / item.duration).min(1.0);
			let eased_ratio = 1.0 - (1.0 - linear_ratio) * (1.0 - linear_ratio);
			let label_node_id = item.node_id;
			let label_position = Vector2::create(item.start_position.x, item.start_position.y - item.rise * eased_ratio);
			let label_node = match node_tree.get_node_mut(label_node_id) {
				Some(label_node) => {
					label_node
				}
				None => {
					// 밖에서 지운 라벨은 목록에서만 뺀다.
					self.active_list.remove(item_index);
					continue;
				}
			};
			label_node.set_local_position(label_position);
			label_node.set_local_opacity(1.0 - linear_ratio * linear_ratio);
			if linear_ratio >= 1.0 {
				label_node.set_active(false);
				self.free_list.push(label_node_id);
				self.active_list.remove(item_index);
			}
		}
	}
}

//==============================================================================
// 시험.
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	// 띄우면 떠오르며 흐려지고, 끝나면 꺼져서 다음 spawn 이 그 노드를 다시 쓴다.
	//==============================================================================
	#[test]
	fn spawn_rises_fades_and_reuses() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let floating_text_id = FloatingText::create(&mut node_tree, root_id);
		let options = FloatingTextOptions {
			jitter: 0.0,
			duration: 1.0,
			rise: 40.0,
			..FloatingTextOptions::default()
		};
		let label_id = FloatingText::spawn(&mut node_tree, floating_text_id, "-120", Vector2::create(100.0, 200.0), options).expect("label");
		assert_eq!(node_tree[label_id].get_component::<Text>().expect("text").get_text(), "-120");
		assert_eq!(node_tree[floating_text_id].get_component::<FloatingText>().expect("floating text").get_active_count(), 1);

		node_tree.tick(&engine, 0.5);
		let label_position = node_tree[label_id].get_local_position();
		assert_eq!(label_position.x, 100.0);
		assert!((label_position.y - (200.0 - 40.0 * 0.75)).abs() < 0.001);
		assert!((node_tree[label_id].get_local_opacity() - 0.75).abs() < 0.001);

		node_tree.tick(&engine, 0.5);
		assert!(!node_tree[label_id].is_active());
		assert_eq!(node_tree[floating_text_id].get_component::<FloatingText>().expect("floating text").get_active_count(), 0);

		let reused_id = FloatingText::spawn(&mut node_tree, floating_text_id, "+5", Vector2::create(0.0, 0.0), options).expect("label");
		assert_eq!(reused_id, label_id);
		assert!(node_tree[reused_id].is_active());
		assert_eq!(node_tree.get_child_count(floating_text_id), 1);

		FloatingText::clear(&mut node_tree, floating_text_id);
		assert!(!node_tree[reused_id].is_active());
		assert_eq!(node_tree[floating_text_id].get_component::<FloatingText>().expect("floating text").get_active_count(), 0);
	}
}
