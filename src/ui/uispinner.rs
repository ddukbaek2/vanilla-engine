//==============================================================================
//! 회전 로딩 표시. (원본 ui/uispinner.js 의 UISpinner)
//! - 원 둘레에 점 여러 개를 놓고, 밝기가 꼬리처럼 돌아가는 표준 스피너. (점 투명도 0.15 ~ 1)
//! - 원본은 WorldNode 를 상속한 노드다. 러스트 판은 노드(호스트)에 붙는 컴포넌트이고, 붙을 때 원본 생성자처럼
//!   호스트를 꾸미고(가운데 피봇, 크기 = (반지름 + 점 반지름) x 2) 점 노드(둥근 Paint)를 자식으로 만든다.
//!   UISpinner::create(node_tree, parent_id, options) 가 원본 new UISpinner(options) + add_child 자리다.
//! - 사용:
//!   ```text
//!     let spinner_id = UISpinner::create(node_tree, panel_id, UISpinnerOptions { radius: 9.0, dot_count: 10, ..UISpinnerOptions::default() });
//!     node_tree[spinner_id].set_local_position(Vector2::create(x, y));
//!   ```
//! - 원본과 다른 것:
//!   - set_dot_color 는 색을 정해 두고 다음 tick 에 점 노드에 건다. (점 노드는 씬 코드가 이 위젯을 쥔 동안 닿을 수 없다)
//!   - 위상(phase)은 0 ~ 1 로 접어 둔다. 꼬리 계산이 1 주기라 보이는 결과는 같고, 오래 돌아도 정밀도가 떨어지지 않는다.
//!   - 점 노드 이름은 원본처럼 "SpinnerDot{번호}" 다. options 객체는 UISpinnerOptions 구조체다.
//==============================================================================
use crate::base::color::Color;
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};

//==============================================================================
/// 스피너 설정. (원본 options = { radius = 9, dot_count = 10, dot_radius = 2, color?, rotation_speed = 1 })
///
/// # 예제
/// 점 넷짜리 스피너
/// ```rust
/// use vanilla::{NodeTree, UISpinner, UISpinnerOptions, Vector2};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let options = UISpinnerOptions {
/// 	dot_count: 4,
/// 	..UISpinnerOptions::default()
/// };
/// let spinner_id = UISpinner::create(&mut node_tree, root_id, options);
/// assert_eq!(node_tree[spinner_id].get_content_size(), Vector2::create(22.0, 22.0));
/// assert_eq!(node_tree[spinner_id].get_component::<UISpinner>().expect("spinner").get_dot_nodes().len(), 4);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UISpinnerOptions {
	/// 점이 놓이는 원의 반지름.
	pub radius: f64,
	/// 점 수. (기본 10)
	pub dot_count: usize,
	/// 점 하나의 반지름. (기본 2)
	pub dot_radius: f64,
	/// 점 색. (기본 흰색)
	pub color: Color,
	/// 초당 바퀴 수.
	pub rotation_speed: f64,
}

impl Default for UISpinnerOptions {
	//==============================================================================
	/// 기본값. (원본과 같다)
	//==============================================================================
	fn default() -> UISpinnerOptions {
		UISpinnerOptions {
			radius: 9.0,
			dot_count: 10,
			dot_radius: 2.0,
			color: Color::white(),
			rotation_speed: 1.0,
		}
	}
}

//==============================================================================
/// 회전 로딩 표시.
///
/// # 예제
/// 판 위에 띄운 스피너
/// ```rust
/// use vanilla::{Engine, EngineConfiguration, NodeTree, UISpinner, UISpinnerOptions, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let panel_id = node_tree.create_child(root_id, "Panel");
/// let spinner_id = UISpinner::create(&mut node_tree, panel_id, UISpinnerOptions { radius: 9.0, dot_count: 10, ..UISpinnerOptions::default() });
/// node_tree[spinner_id].set_local_position(Vector2::create(120.0, 80.0));
/// node_tree[spinner_id].get_component_mut::<UISpinner>().expect("spinner").set_dot_color("#ffcc00");
/// node_tree.tick(&engine, 0.1);
/// let dot_ids = node_tree[spinner_id].get_component::<UISpinner>().expect("spinner").get_dot_nodes().to_vec();
/// assert!((node_tree[dot_ids[1]].get_local_opacity() - 1.0).abs() < 0.0001);
/// ```
//==============================================================================
pub struct UISpinner {
	component_base: ComponentBase,
	options: UISpinnerOptions,
	dot_node_ids: Vec<NodeId>,
	rotation_speed: f64,
	phase: f64,
	dot_color: Color,
	is_dot_color_pending: bool,
}

#[crate::reflect]
impl UISpinner {
	//==============================================================================
	/// 생성. (기본 설정)
	///
	/// # 돌려줌
	/// 붙기 전의 스피너. (UISpinnerOptions::default)
	//==============================================================================
	pub fn new() -> UISpinner {
		UISpinner::create_with_options(UISpinnerOptions::default())
	}

	//==============================================================================
	/// 설정으로 생성. (노드에는 node_tree.add_component 로 붙인다)
	///
	/// # 인자
	/// - `options`: 스피너 설정.
	///
	/// # 돌려줌
	/// 붙기 전의 스피너.
	//==============================================================================
	pub fn create_with_options(options: UISpinnerOptions) -> UISpinner {
		UISpinner {
			component_base: ComponentBase::new(),
			options,
			dot_node_ids: Vec::new(),
			rotation_speed: options.rotation_speed,
			phase: 0.0,
			dot_color: options.color,
			is_dot_color_pending: false,
		}
	}

	//==============================================================================
	/// 노드 만들기. (parent_id 밑에 "UISpinner" 노드를 만들어 붙인다. 원본 new UISpinner(options) + add_child)
	///
	/// # 인자
	/// - `node_tree`: 노드 트리.
	/// - `parent_id`: 부모 노드.
	/// - `options`: 스피너 설정.
	///
	/// # 돌려줌
	/// 만든 스피너 노드. (가운데 피봇, 크기는 (반지름 + 점 반지름) x 2)
	//==============================================================================
	pub fn create(node_tree: &mut NodeTree, parent_id: NodeId, options: UISpinnerOptions) -> NodeId {
		let spinner_id = node_tree.create_child(parent_id, "UISpinner");
		node_tree.add_component(spinner_id, UISpinner::create_with_options(options));
		spinner_id
	}

	//==============================================================================
	/// 점 투명도 계산. (점 순서 비율에서 위상을 뺀 꼬리 비율이 작을수록 밝다)
	/// - 점이 없으면(dot_count 0) 순서 비율을 0 으로 본다.
	///
	/// # 인자
	/// - `dot_index`: 점 번호. (0 부터, 0 번은 원의 오른쪽이고 번호가 늘수록 시계 방향)
	///
	/// # 돌려줌
	/// 점 투명도. (0.15 ~ 1)
	///
	/// # 예제
	/// 위상 0 에서는 0 번 점이 가장 밝다
	/// ```rust
	/// use vanilla::UISpinner;
	///
	/// let spinner = UISpinner::new();
	/// assert!((spinner.calculate_dot_opacity(0) - 1.0).abs() < 0.0001);
	/// assert!((spinner.calculate_dot_opacity(5) - 0.575).abs() < 0.0001);
	/// ```
	//==============================================================================
	pub fn calculate_dot_opacity(&self, dot_index: usize) -> f64 {
		let order_ratio = if self.options.dot_count > 0 {
			dot_index as f64 / self.options.dot_count as f64
		}
		else {
			0.0
		};
		let mut tail_ratio = (order_ratio - self.phase) % 1.0;
		if tail_ratio < 0.0 {
			tail_ratio += 1.0;
		}
		0.15 + 0.85 * (1.0 - tail_ratio)
	}

	//==============================================================================
	/// 점 색 설정. (다음 tick 에 점 노드에 건다)
	///
	/// # 인자
	/// - `dot_color`: 점 색. (`"#ff0000"` 같은 글이나 Color)
	//==============================================================================
	pub fn set_dot_color(&mut self, dot_color: impl Into<Color>) {
		self.dot_color = dot_color.into();
		self.is_dot_color_pending = true;
	}

	//==============================================================================
	/// 점 색 반환.
	///
	/// # 돌려줌
	/// 점 색. (처음은 설정의 color)
	//==============================================================================
	pub fn get_dot_color(&self) -> Color {
		self.dot_color
	}

	//==============================================================================
	/// 회전 속도 설정. (초당 바퀴 수)
	///
	/// # 인자
	/// - `rotation_speed`: 초당 바퀴 수. (음수면 반대로 돈다)
	//==============================================================================
	pub fn set_rotation_speed(&mut self, rotation_speed: f64) {
		self.rotation_speed = rotation_speed;
	}

	//==============================================================================
	/// 회전 속도 반환.
	///
	/// # 돌려줌
	/// 초당 바퀴 수. (처음은 설정의 rotation_speed, 기본 1)
	//==============================================================================
	pub fn get_rotation_speed(&self) -> f64 {
		self.rotation_speed
	}

	//==============================================================================
	/// 점 노드 핸들 목록.
	///
	/// # 돌려줌
	/// 점 노드. (번호 차례, 트리에 붙기 전이면 빈 목록)
	//==============================================================================
	pub fn get_dot_nodes(&self) -> &[NodeId] {
		&self.dot_node_ids
	}
}

impl Default for UISpinner {
	//==============================================================================
	/// 기본값. (UISpinner::new 와 같다)
	//==============================================================================
	fn default() -> UISpinner {
		UISpinner::new()
	}
}

#[crate::reflect]
impl Component for UISpinner {
	//==============================================================================
	/// 컴포넌트 공통 칸 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 컴포넌트 공통 칸 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 타입 이름. ("UISpinner")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UISpinner"
	}

	//==============================================================================
	/// 트리에 붙음. (원본 생성자: 가운데 피봇, 크기, 둘레의 점 노드)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		let options = self.options;
		let center = options.radius + options.dot_radius;
		let spinner_node = &mut node_tree[node_id];
		spinner_node.set_pivot(Pivot::MIDDLE_CENTER);
		spinner_node.set_anchor(Pivot::TOP_LEFT);
		spinner_node.set_content_size(Vector2::create(center * 2.0, center * 2.0));
		for dot_index in 0..options.dot_count {
			// 원본처럼 f64 로 셈한다.
			let angle = (dot_index as f64 / options.dot_count as f64) * std::f64::consts::PI * 2.0;
			let dot_name = format!("SpinnerDot{}", dot_index);
			let dot_id = node_tree.create_child(node_id, &dot_name);
			let dot_node = &mut node_tree[dot_id];
			dot_node.set_pivot(Pivot::MIDDLE_CENTER);
			dot_node.set_anchor(Pivot::TOP_LEFT);
			dot_node.set_content_size(Vector2::create(options.dot_radius * 2.0, options.dot_radius * 2.0));
			let dot_x = center + angle.cos() * options.radius;
			let dot_y = center + angle.sin() * options.radius;
			dot_node.set_local_position(Vector2::create(dot_x, dot_y));
			let paint = dot_node.add_component(Paint::create_with_color(self.dot_color));
			paint.set_round_size(options.dot_radius);
			self.dot_node_ids.push(dot_id);
		}
	}

	//==============================================================================
	/// 갱신. (위상을 돌리고 점마다 투명도를 건다. 미뤄 둔 점 색도 건다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
		self.phase += time_delta * self.rotation_speed;
		self.phase -= self.phase.floor();
		let is_dot_color_pending = self.is_dot_color_pending;
		self.is_dot_color_pending = false;
		let node_tree = context.get_node_tree_mut();
		for (dot_index, dot_node_id) in self.dot_node_ids.iter().copied().enumerate() {
			let dot_opacity = self.calculate_dot_opacity(dot_index);
			let dot_node = match node_tree.get_node_mut(dot_node_id) {
				Some(dot_node) => {
					dot_node
				}
				None => {
					continue;
				}
			};
			dot_node.set_local_opacity(dot_opacity);
			if is_dot_color_pending {
				if let Some(paint) = dot_node.get_component_mut::<Paint>() {
					paint.set_color(self.dot_color);
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};

	//==============================================================================
	/// 점은 원 둘레에 놓이고, 꼬리 밝기가 위상을 따라 돈다. 점 색은 다음 tick 에 걸린다.
	//==============================================================================
	#[test]
	fn dots_circle_and_tail_rotates() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let options = UISpinnerOptions {
			dot_count: 4,
			..UISpinnerOptions::default()
		};
		let spinner_id = UISpinner::create(&mut node_tree, root_id, options);
		assert_eq!(node_tree[spinner_id].get_content_size(), Vector2::create(22.0, 22.0));
		let dot_ids = node_tree[spinner_id].get_component::<UISpinner>().expect("spinner").get_dot_nodes().to_vec();
		assert_eq!(dot_ids.len(), 4);
		assert_eq!(node_tree[dot_ids[0]].get_local_position(), Vector2::create(20.0, 11.0));
		assert_eq!(node_tree[dot_ids[0]].get_content_size(), Vector2::create(4.0, 4.0));
		assert_eq!(node_tree[dot_ids[1]].get_name(), "SpinnerDot1");

		node_tree.tick(&engine, 0.25);
		let expected_opacities = [0.3625, 1.0, 0.7875, 0.575];
		for (dot_index, dot_id) in dot_ids.iter().enumerate() {
			assert!((node_tree[*dot_id].get_local_opacity() - expected_opacities[dot_index]).abs() < 0.0001);
		}

		// 한 바퀴를 넘겨도 같은 위상이면 같은 밝기다.
		node_tree.tick(&engine, 1.0);
		assert!((node_tree[dot_ids[1]].get_local_opacity() - 1.0).abs() < 0.0001);

		node_tree[spinner_id].get_component_mut::<UISpinner>().expect("spinner").set_dot_color("#ff0000");
		assert_eq!(node_tree[dot_ids[2]].get_component::<Paint>().expect("paint").get_color(), Color::white());
		node_tree.tick(&engine, 0.016);
		assert_eq!(node_tree[dot_ids[2]].get_component::<Paint>().expect("paint").get_color(), Color::create(1.0, 0.0, 0.0, 1.0));
	}

	//==============================================================================
	/// 점이 없는 스피너도 점 투명도가 0 나누기로 NaN 이 되지 않는다.
	//==============================================================================
	#[test]
	fn zero_dot_count_opacity_is_finite() {
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let options = UISpinnerOptions {
			dot_count: 0,
			..UISpinnerOptions::default()
		};
		let spinner_id = UISpinner::create(&mut node_tree, root_id, options);
		let spinner = node_tree[spinner_id].get_component::<UISpinner>().expect("spinner");
		assert!(spinner.get_dot_nodes().is_empty());
		assert_eq!(spinner.calculate_dot_opacity(0), 1.0);
	}
}
