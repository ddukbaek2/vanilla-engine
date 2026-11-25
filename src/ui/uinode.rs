//==============================================================================
// UI 노드. (원본 ui/uinode.js 의 UINode)
// - 영역을 솔버 변수 8 개(left, right, top, bottom, width, height, 파생 center_x, center_y)로 나타내고, 사용자가 LayoutSolver 에 제약을 걸어
//   그 값들을 정한다. 본질 제약 4 개(right == left + width, bottom == top + height, center_x == left + width / 2, center_y == top + height / 2)는 스스로 등록한다.
// - 사용자 제약은 add_constraint 로 등록한다. 솔버는 set_solver 로 붙인다. (UISceneBase 는 트리의 UINode 에 씬의 솔버를 스스로 붙인다)
// - intrinsic content size, content hugging, compression resistance, layout margins, layout margins guide 는 UIKit 의 같은 이름 API 와 같은 생각이다.
// - 솔버 변수(left, top)는 UI 트리 뿌리가 놓인 좌표계 기준의 절대 위치다. 부모도 UINode 면 자식은 (left - 부모 left, top - 부모 top) 만큼 떨어져 놓인다.
//   회전, 배율, 투명도는 노드의 것을 그대로 쓴다.
// - 사용:
//     let node_id = node_tree.create_child(parent_id, "title");
//     node_tree.add_component(node_id, UINode::new());
//     node_tree.with_component_mut::<UINode, _>(node_id, |ui_node, _tree| {
//         ui_node.add_constraint(&ui_node.left_anchor().equal_to(screen_node.left_anchor().add(16))).unwrap();
//     });
// - 원본과 다른 것:
//   - 원본은 TransformNode 를 상속한 따로 된 노드이고 pushTransform 에서 솔버 값으로 옮긴다. 러스트 판은 노드가 WorldNode 하나뿐이라
//     WorldNode 에 붙는 컴포넌트이고, 솔버 값을 노드의 자리(local_position, 피봇과 앵커는 왼쪽 위)와 content_size 로 옮긴다.
//     옮기는 때는 컴포넌트 tick 과 apply_layout_in_tree(UISceneBase 가 솔버를 푼 바로 뒤에 부른다)다. 원본은 그릴 때마다 읽는다.
//   - 원본 getIntrinsicContentSize 는 노드의 컴포넌트 가운데 그 함수가 있는 것(Text, RichText)의 값을 모은다. 러스트 판은 Component::get_intrinsic_content_size 를 모은다.
//     원본 UILabel 은 글이 바뀔 때 노드의 invalidateIntrinsicContentSize 를 부른다. 러스트 판의 컴포넌트는 노드의 다른 컴포넌트를 부를 수 없어서,
//     UINode 가 tick(과 UISceneBase 의 솔버 붙이기)마다 컴포넌트들의 값을 다시 읽어 바뀌었으면 스스로 invalidate 한다.
//   - 원본이 예외를 던지는 곳(솔버의 겹친 제약, 만족할 수 없는 required 제약)은 Result<(), LayoutError> 를 돌려준다.
//     (add_constraint, remove_constraint, set_solver, set_layout_margins, invalidate_intrinsic_content_size)
//   - 앵커는 원본의 게터 속성(leftAnchor) 대신 함수(left_anchor)다. 축 인자는 문자열 대신 LayoutConstraintAxis 열거라 "지원되지 않는 축" 예외가 없다.
//   - layout margins guide 는 원본처럼 트리에 들지 않는 UINode 다. 러스트 판은 이 UINode 가 Box 로 품는다.
//==============================================================================
use crate::base::pivot::Pivot;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::{NodeTransform, WorldNode};
use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
use crate::ui::autolayout::layoutconstraintaxis::LayoutConstraintAxis;
use crate::ui::autolayout::layoutexpression::LayoutExpression;
use crate::ui::autolayout::layoutpriority::LayoutPriority;
use crate::ui::autolayout::layoutsolver::{LayoutError, LayoutSolver};
use crate::ui::autolayout::layoutstrength::LayoutStrength;
use crate::ui::autolayout::layoutvariable::LayoutVariable;

//==============================================================================
// intrinsic 선호가 없다는 값. (UIKit 의 UIView.noIntrinsicMetric, 원본 UINode.noIntrinsicMetric)
//==============================================================================
pub const NO_INTRINSIC_METRIC: f32 = -1.0;

//==============================================================================
// 레이아웃 여백. (UIKit 의 UIView.layoutMargins, 원본 { top, left, bottom, right })
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutMargins {
	pub top: f64,
	pub left: f64,
	pub bottom: f64,
	pub right: f64,
}

impl LayoutMargins {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn create(top: f64, left: f64, bottom: f64, right: f64) -> LayoutMargins {
		LayoutMargins {
			top,
			left,
			bottom,
			right,
		}
	}
}

//==============================================================================
// UI 노드.
//==============================================================================
pub struct UINode {
	component_base: ComponentBase,
	solver: Option<LayoutSolver>,
	left_variable: LayoutVariable,
	right_variable: LayoutVariable,
	top_variable: LayoutVariable,
	bottom_variable: LayoutVariable,
	width_variable: LayoutVariable,
	height_variable: LayoutVariable,
	center_x_variable: LayoutVariable,
	center_y_variable: LayoutVariable,
	intrinsic_constraints: Vec<LayoutConstraint>,
	user_constraints: Vec<LayoutConstraint>,
	intrinsic_size_constraints: Vec<LayoutConstraint>,
	horizontal_hugging_priority: f64,
	vertical_hugging_priority: f64,
	horizontal_compression_resistance_priority: f64,
	vertical_compression_resistance_priority: f64,
	layout_margins: LayoutMargins,
	layout_margins_guide: Option<Box<UINode>>,
	layout_margins_guide_constraints: Vec<LayoutConstraint>,
	// 마지막으로 읽은 노드 컴포넌트들의 intrinsic content size.
	intrinsic_content_size: Vector2,
}

#[crate::reflect]
impl UINode {
	//==============================================================================
	// 생성. (본질 제약 4 개를 만든다, hug 250, 압축 저항 750, 여백 8)
	//==============================================================================
	pub fn new() -> UINode {
		let mut ui_node = UINode {
			component_base: ComponentBase::new(),
			solver: None,
			left_variable: LayoutVariable::new("left"),
			right_variable: LayoutVariable::new("right"),
			top_variable: LayoutVariable::new("top"),
			bottom_variable: LayoutVariable::new("bottom"),
			width_variable: LayoutVariable::new("width"),
			height_variable: LayoutVariable::new("height"),
			center_x_variable: LayoutVariable::new("centerX"),
			center_y_variable: LayoutVariable::new("centerY"),
			intrinsic_constraints: Vec::new(),
			user_constraints: Vec::new(),
			intrinsic_size_constraints: Vec::new(),
			horizontal_hugging_priority: LayoutPriority::DEFAULT_LOW,
			vertical_hugging_priority: LayoutPriority::DEFAULT_LOW,
			horizontal_compression_resistance_priority: LayoutPriority::DEFAULT_HIGH,
			vertical_compression_resistance_priority: LayoutPriority::DEFAULT_HIGH,
			layout_margins: LayoutMargins::create(8.0, 8.0, 8.0, 8.0),
			layout_margins_guide: None,
			layout_margins_guide_constraints: Vec::new(),
			intrinsic_content_size: Vector2::create(NO_INTRINSIC_METRIC, NO_INTRINSIC_METRIC),
		};
		ui_node.intrinsic_constraints = ui_node.build_intrinsic_constraints();
		ui_node
	}

	//==============================================================================
	// 컨텐트 크기 반환. (솔버가 푼 width, height)
	//==============================================================================
	pub fn get_content_size(&self) -> Vector2 {
		Vector2::create(self.get_width() as f32, self.get_height() as f32)
	}

	//==============================================================================
	// 본질 제약 4 개 생성.
	// - right == left + width, bottom == top + height, center_x == left + width / 2, center_y == top + height / 2
	//==============================================================================
	pub fn build_intrinsic_constraints(&self) -> Vec<LayoutConstraint> {
		let right_constraint = self.right_anchor().equal_to(self.left_anchor().add(self.width_anchor()));
		let bottom_constraint = self.bottom_anchor().equal_to(self.top_anchor().add(self.height_anchor()));
		let center_x_constraint = self.center_x_anchor().equal_to(self.left_anchor().add(self.width_anchor().divide(2.0)));
		let center_y_constraint = self.center_y_anchor().equal_to(self.top_anchor().add(self.height_anchor().divide(2.0)));
		vec![right_constraint, bottom_constraint, center_x_constraint, center_y_constraint]
	}

	//==============================================================================
	// intrinsic content size 기반 제약(hug, compression resistance) 생성. (축의 값이 NO_INTRINSIC_METRIC 이면 그 축은 만들지 않는다)
	// - hug: width <= intrinsic_width @ hugging, compression: width >= intrinsic_width @ compression resistance
	//==============================================================================
	pub fn build_intrinsic_size_constraints(&self) -> Vec<LayoutConstraint> {
		let intrinsic_size = self.get_intrinsic_content_size();
		let mut result = Vec::new();
		if intrinsic_size.x != NO_INTRINSIC_METRIC {
			let horizontal_hugging_strength = LayoutStrength::from_priority(self.horizontal_hugging_priority);
			let horizontal_compression_strength = LayoutStrength::from_priority(self.horizontal_compression_resistance_priority);
			result.push(self.width_anchor().less_than_or_equal_to(intrinsic_size.x as f64).with_strength(horizontal_hugging_strength));
			result.push(self.width_anchor().greater_than_or_equal_to(intrinsic_size.x as f64).with_strength(horizontal_compression_strength));
		}
		if intrinsic_size.y != NO_INTRINSIC_METRIC {
			let vertical_hugging_strength = LayoutStrength::from_priority(self.vertical_hugging_priority);
			let vertical_compression_strength = LayoutStrength::from_priority(self.vertical_compression_resistance_priority);
			result.push(self.height_anchor().less_than_or_equal_to(intrinsic_size.y as f64).with_strength(vertical_hugging_strength));
			result.push(self.height_anchor().greater_than_or_equal_to(intrinsic_size.y as f64).with_strength(vertical_compression_strength));
		}
		result
	}

	//==============================================================================
	// intrinsic content size 반환. (노드 컴포넌트들의 값의 큰 쪽, 마지막으로 읽은 값. 축마다 NO_INTRINSIC_METRIC 이면 선호 없음)
	//==============================================================================
	pub fn get_intrinsic_content_size(&self) -> Vector2 {
		self.intrinsic_content_size
	}

	//==============================================================================
	// 노드 컴포넌트들의 intrinsic content size 모으기. (원본 getIntrinsicContentSize 기본 구현, 정적)
	//==============================================================================
	pub fn collect_intrinsic_content_size(node: &WorldNode) -> Vector2 {
		let mut maximum_width = NO_INTRINSIC_METRIC;
		let mut maximum_height = NO_INTRINSIC_METRIC;
		for component in node.get_all_components().iter() {
			let component_size = component.get_intrinsic_content_size();
			if component_size.x > maximum_width {
				maximum_width = component_size.x;
			}
			if component_size.y > maximum_height {
				maximum_height = component_size.y;
			}
		}
		Vector2::create(maximum_width, maximum_height)
	}

	//==============================================================================
	// intrinsic content size 를 바꾸고 바뀌었으면 제약을 다시 건다. (UISceneBase 와 tick 이 노드 컴포넌트에서 읽어 부른다)
	//==============================================================================
	pub fn set_intrinsic_content_size(&mut self, intrinsic_content_size: Vector2) -> Result<(), LayoutError> {
		if self.intrinsic_content_size == intrinsic_content_size {
			return Ok(());
		}
		self.intrinsic_content_size = intrinsic_content_size;
		self.invalidate_intrinsic_content_size()
	}

	//==============================================================================
	// intrinsic content size 무효화. (UIKit 의 invalidateIntrinsicContentSize, 솔버의 옛 제약을 빼고 새 값으로 다시 건다)
	//==============================================================================
	pub fn invalidate_intrinsic_content_size(&mut self) -> Result<(), LayoutError> {
		if let Some(solver) = self.solver.as_ref() {
			for constraint in self.intrinsic_size_constraints.iter() {
				if solver.has_constraint(constraint) {
					solver.remove_constraint(constraint)?;
				}
			}
		}
		self.intrinsic_size_constraints = self.build_intrinsic_size_constraints();
		if let Some(solver) = self.solver.as_ref() {
			for constraint in self.intrinsic_size_constraints.iter() {
				solver.add_constraint(constraint)?;
			}
		}
		Ok(())
	}

	//==============================================================================
	// content hugging 우선순위 반환. (UIKit 의 contentHuggingPriority(for:))
	//==============================================================================
	pub fn get_content_hugging_priority(&self, axis: LayoutConstraintAxis) -> f64 {
		match axis {
			LayoutConstraintAxis::Horizontal => {
				self.horizontal_hugging_priority
			}
			LayoutConstraintAxis::Vertical => {
				self.vertical_hugging_priority
			}
		}
	}

	//==============================================================================
	// content hugging 우선순위 설정. (intrinsic size 제약을 바로 다시 건다)
	//==============================================================================
	pub fn set_content_hugging_priority(&mut self, priority: f64, axis: LayoutConstraintAxis) -> Result<(), LayoutError> {
		match axis {
			LayoutConstraintAxis::Horizontal => {
				self.horizontal_hugging_priority = priority;
			}
			LayoutConstraintAxis::Vertical => {
				self.vertical_hugging_priority = priority;
			}
		}
		self.invalidate_intrinsic_content_size()
	}

	//==============================================================================
	// content compression resistance 우선순위 반환.
	//==============================================================================
	pub fn get_content_compression_resistance_priority(&self, axis: LayoutConstraintAxis) -> f64 {
		match axis {
			LayoutConstraintAxis::Horizontal => {
				self.horizontal_compression_resistance_priority
			}
			LayoutConstraintAxis::Vertical => {
				self.vertical_compression_resistance_priority
			}
		}
	}

	//==============================================================================
	// content compression resistance 우선순위 설정. (intrinsic size 제약을 바로 다시 건다)
	//==============================================================================
	pub fn set_content_compression_resistance_priority(&mut self, priority: f64, axis: LayoutConstraintAxis) -> Result<(), LayoutError> {
		match axis {
			LayoutConstraintAxis::Horizontal => {
				self.horizontal_compression_resistance_priority = priority;
			}
			LayoutConstraintAxis::Vertical => {
				self.vertical_compression_resistance_priority = priority;
			}
		}
		self.invalidate_intrinsic_content_size()
	}

	//==============================================================================
	// layout margins 반환. (UIKit 의 UIView.layoutMargins)
	//==============================================================================
	pub fn get_layout_margins(&self) -> LayoutMargins {
		self.layout_margins
	}

	//==============================================================================
	// layout margins 설정. (가이드가 이미 있으면 가이드 제약을 바로 다시 건다)
	//==============================================================================
	pub fn set_layout_margins(&mut self, layout_margins: LayoutMargins) -> Result<(), LayoutError> {
		self.layout_margins = layout_margins;
		if self.layout_margins_guide.is_some() {
			return self.rebuild_layout_margins_guide_constraints();
		}
		Ok(())
	}

	//==============================================================================
	// layout margins guide 반환. (UIKit 의 UIView.layoutMarginsGuide, 처음 부를 때 만들고 여백 제약 4 개를 건다)
	// - 가이드는 트리에 들지 않아 그려지지 않고 앵커만 쓴다. 여백 제약이 솔버와 부딪히면 가이드는 만들되 그 제약은 걸리지 않는다.
	//==============================================================================
	pub fn get_layout_margins_guide(&mut self) -> &UINode {
		if self.layout_margins_guide.is_none() {
			let mut layout_margins_guide = UINode::new();
			if let Some(solver) = self.solver.as_ref() {
				let _ = layout_margins_guide.set_solver(Some(solver.clone()));
			}
			self.layout_margins_guide = Some(Box::new(layout_margins_guide));
			let _ = self.rebuild_layout_margins_guide_constraints();
		}
		self.layout_margins_guide.as_deref().expect("layout margins guide")
	}

	//==============================================================================
	// layout margins guide 의 여백 제약 4 개를 다시 짓는다. (옛 제약을 빼고 지금 여백으로 지어 다시 건다)
	//==============================================================================
	pub fn rebuild_layout_margins_guide_constraints(&mut self) -> Result<(), LayoutError> {
		let guide = match self.layout_margins_guide.as_ref() {
			Some(guide) => {
				guide
			}
			None => {
				return Ok(());
			}
		};
		if let Some(solver) = self.solver.as_ref() {
			for constraint in self.layout_margins_guide_constraints.iter() {
				if solver.has_constraint(constraint) {
					solver.remove_constraint(constraint)?;
				}
			}
		}
		let margins = self.layout_margins;
		let guide_left_constraint = guide.left_anchor().equal_to(self.left_anchor().add(margins.left));
		let guide_top_constraint = guide.top_anchor().equal_to(self.top_anchor().add(margins.top));
		let guide_right_constraint = guide.right_anchor().equal_to(self.right_anchor().subtract(margins.right));
		let guide_bottom_constraint = guide.bottom_anchor().equal_to(self.bottom_anchor().subtract(margins.bottom));
		self.layout_margins_guide_constraints = vec![guide_left_constraint, guide_top_constraint, guide_right_constraint, guide_bottom_constraint];
		if let Some(solver) = self.solver.as_ref() {
			for constraint in self.layout_margins_guide_constraints.iter() {
				solver.add_constraint(constraint)?;
			}
		}
		Ok(())
	}

	//==============================================================================
	// 솔버 설정, 변경, 해제. (옛 솔버에서 이 노드의 제약을 모두 빼고 새 솔버에 다시 건다, 가이드도 함께 옮긴다. None 이면 빼기만 한다)
	//==============================================================================
	pub fn set_solver(&mut self, solver: Option<LayoutSolver>) -> Result<(), LayoutError> {
		if let Some(previous_solver) = self.solver.as_ref() {
			let constraint_groups = [&self.layout_margins_guide_constraints, &self.user_constraints, &self.intrinsic_size_constraints, &self.intrinsic_constraints];
			for constraint_group in constraint_groups {
				for constraint in constraint_group.iter() {
					if previous_solver.has_constraint(constraint) {
						previous_solver.remove_constraint(constraint)?;
					}
				}
			}
		}
		self.solver = solver;
		if let Some(layout_margins_guide) = self.layout_margins_guide.as_mut() {
			layout_margins_guide.set_solver(self.solver.clone())?;
		}
		if let Some(solver) = self.solver.as_ref() {
			let constraint_groups = [&self.intrinsic_constraints, &self.intrinsic_size_constraints, &self.user_constraints, &self.layout_margins_guide_constraints];
			for constraint_group in constraint_groups {
				for constraint in constraint_group.iter() {
					solver.add_constraint(constraint)?;
				}
			}
		}
		Ok(())
	}

	//==============================================================================
	// 솔버 반환.
	//==============================================================================
	pub fn get_solver(&self) -> Option<&LayoutSolver> {
		self.solver.as_ref()
	}

	//==============================================================================
	// 사용자 제약 추가. (솔버가 있으면 바로 건다)
	//==============================================================================
	pub fn add_constraint(&mut self, constraint: &LayoutConstraint) -> Result<(), LayoutError> {
		self.user_constraints.push(constraint.clone());
		if let Some(solver) = self.solver.as_ref() {
			solver.add_constraint(constraint)?;
		}
		Ok(())
	}

	//==============================================================================
	// 사용자 제약 빼기. (솔버에서도 뺀다)
	//==============================================================================
	pub fn remove_constraint(&mut self, constraint: &LayoutConstraint) -> Result<(), LayoutError> {
		if let Some(index) = self.user_constraints.iter().position(|candidate| candidate == constraint) {
			self.user_constraints.remove(index);
		}
		if let Some(solver) = self.solver.as_ref() {
			if solver.has_constraint(constraint) {
				solver.remove_constraint(constraint)?;
			}
		}
		Ok(())
	}

	//==============================================================================
	// 등록한 사용자 제약 목록. (사본)
	//==============================================================================
	pub fn get_constraints(&self) -> Vec<LayoutConstraint> {
		self.user_constraints.clone()
	}

	//==============================================================================
	// 왼쪽 변수 반환. (식 밖의 쓰임에 변수가 필요할 때)
	//==============================================================================
	pub fn get_left_variable(&self) -> &LayoutVariable {
		&self.left_variable
	}

	//==============================================================================
	// 오른쪽 변수 반환.
	//==============================================================================
	pub fn get_right_variable(&self) -> &LayoutVariable {
		&self.right_variable
	}

	//==============================================================================
	// 위 변수 반환.
	//==============================================================================
	pub fn get_top_variable(&self) -> &LayoutVariable {
		&self.top_variable
	}

	//==============================================================================
	// 아래 변수 반환.
	//==============================================================================
	pub fn get_bottom_variable(&self) -> &LayoutVariable {
		&self.bottom_variable
	}

	//==============================================================================
	// 너비 변수 반환.
	//==============================================================================
	pub fn get_width_variable(&self) -> &LayoutVariable {
		&self.width_variable
	}

	//==============================================================================
	// 높이 변수 반환.
	//==============================================================================
	pub fn get_height_variable(&self) -> &LayoutVariable {
		&self.height_variable
	}

	//==============================================================================
	// 가로 중심 변수 반환.
	//==============================================================================
	pub fn get_center_x_variable(&self) -> &LayoutVariable {
		&self.center_x_variable
	}

	//==============================================================================
	// 세로 중심 변수 반환.
	//==============================================================================
	pub fn get_center_y_variable(&self) -> &LayoutVariable {
		&self.center_y_variable
	}

	//==============================================================================
	// 왼쪽 앵커 식. (UIKit 의 leftAnchor, 부를 때마다 새 식)
	//==============================================================================
	pub fn left_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.left_variable)
	}

	//==============================================================================
	// 오른쪽 앵커 식.
	//==============================================================================
	pub fn right_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.right_variable)
	}

	//==============================================================================
	// 위 앵커 식.
	//==============================================================================
	pub fn top_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.top_variable)
	}

	//==============================================================================
	// 아래 앵커 식.
	//==============================================================================
	pub fn bottom_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.bottom_variable)
	}

	//==============================================================================
	// 너비 앵커 식.
	//==============================================================================
	pub fn width_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.width_variable)
	}

	//==============================================================================
	// 높이 앵커 식.
	//==============================================================================
	pub fn height_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.height_variable)
	}

	//==============================================================================
	// 가로 중심 앵커 식.
	//==============================================================================
	pub fn center_x_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.center_x_variable)
	}

	//==============================================================================
	// 세로 중심 앵커 식.
	//==============================================================================
	pub fn center_y_anchor(&self) -> LayoutExpression {
		LayoutExpression::from_variable(&self.center_y_variable)
	}

	//==============================================================================
	// 시작 앵커 식. (UIKit 의 leadingAnchor, 원본처럼 오른쪽에서 왼쪽을 다루지 않아 늘 left_anchor)
	//==============================================================================
	pub fn leading_anchor(&self) -> LayoutExpression {
		self.left_anchor()
	}

	//==============================================================================
	// 끝 앵커 식. (UIKit 의 trailingAnchor, 늘 right_anchor)
	//==============================================================================
	pub fn trailing_anchor(&self) -> LayoutExpression {
		self.right_anchor()
	}

	//==============================================================================
	// 첫 기준선 앵커 식. (글이 없는 노드는 top_anchor)
	//==============================================================================
	pub fn first_baseline_anchor(&self) -> LayoutExpression {
		self.top_anchor()
	}

	//==============================================================================
	// 마지막 기준선 앵커 식. (글이 없는 노드는 bottom_anchor)
	//==============================================================================
	pub fn last_baseline_anchor(&self) -> LayoutExpression {
		self.bottom_anchor()
	}

	//==============================================================================
	// 왼쪽 위치. (LayoutSolver::update_variables 뒤에 맞는 값)
	//==============================================================================
	pub fn get_left(&self) -> f64 {
		self.left_variable.get_value()
	}

	//==============================================================================
	// 오른쪽 위치.
	//==============================================================================
	pub fn get_right(&self) -> f64 {
		self.right_variable.get_value()
	}

	//==============================================================================
	// 위 위치.
	//==============================================================================
	pub fn get_top(&self) -> f64 {
		self.top_variable.get_value()
	}

	//==============================================================================
	// 아래 위치.
	//==============================================================================
	pub fn get_bottom(&self) -> f64 {
		self.bottom_variable.get_value()
	}

	//==============================================================================
	// 너비.
	//==============================================================================
	pub fn get_width(&self) -> f64 {
		self.width_variable.get_value()
	}

	//==============================================================================
	// 높이.
	//==============================================================================
	pub fn get_height(&self) -> f64 {
		self.height_variable.get_value()
	}

	//==============================================================================
	// 가로 중심 위치.
	//==============================================================================
	pub fn get_center_x(&self) -> f64 {
		self.center_x_variable.get_value()
	}

	//==============================================================================
	// 세로 중심 위치.
	//==============================================================================
	pub fn get_center_y(&self) -> f64 {
		self.center_y_variable.get_value()
	}

	//==============================================================================
	// 솔버 값을 노드에 옮기기. (자리는 부모 UINode 의 왼쪽 위 기준, 부모가 UINode 가 아니면 left, top 그대로. 크기는 width, height)
	//==============================================================================
	fn apply_to_node(&self, node: &mut WorldNode, parent_left_top: Option<(f64, f64)>) {
		let (parent_left, parent_top) = parent_left_top.unwrap_or((0.0, 0.0));
		node.set_local_position(Vector2::create((self.get_left() - parent_left) as f32, (self.get_top() - parent_top) as f32));
		node.set_content_size(self.get_content_size());
	}

	//==============================================================================
	// 부모 노드의 UINode 왼쪽 위. (부모가 없거나 UINode 가 아니면 None)
	//==============================================================================
	fn read_parent_left_top(node_tree: &NodeTree, node_id: NodeId) -> Option<(f64, f64)> {
		let parent_id = node_tree.get_parent_id(node_id)?;
		let parent_ui_node = node_tree.get_node(parent_id)?.get_component::<UINode>()?;
		Some((parent_ui_node.get_left(), parent_ui_node.get_top()))
	}

	//==============================================================================
	// 노드 아래 모든 UINode 의 솔버 값을 노드에 옮기기. (UISceneBase 가 솔버를 푼 바로 뒤에 부른다, 정적)
	//==============================================================================
	pub fn apply_layout_in_tree(node_tree: &mut NodeTree, node_id: NodeId) {
		let parent_left_top = UINode::read_parent_left_top(node_tree, node_id);
		if let Some(node) = node_tree.get_node_mut(node_id) {
			let layout_values = node.get_component::<UINode>().map(|ui_node| (ui_node.get_left(), ui_node.get_top(), ui_node.get_content_size()));
			if let Some((left, top, content_size)) = layout_values {
				let (parent_left, parent_top) = parent_left_top.unwrap_or((0.0, 0.0));
				node.set_local_position(Vector2::create((left - parent_left) as f32, (top - parent_top) as f32));
				node.set_content_size(content_size);
			}
		}
		let child_ids: Vec<NodeId> = node_tree.get_child_ids(node_id).to_vec();
		for child_id in child_ids {
			UINode::apply_layout_in_tree(node_tree, child_id);
		}
	}

	//==============================================================================
	// 노드 아래 솔버가 없는 UINode 에 솔버를 붙이고 intrinsic content size 를 다시 읽기. (UISceneBase 가 tick 마다 부른다, 원본 attachPendingUINodes)
	//==============================================================================
	pub fn attach_pending_in_tree(node_tree: &mut NodeTree, node_id: NodeId, solver: &LayoutSolver) {
		let has_ui_node = node_tree.get_node(node_id).map(|node| node.get_component::<UINode>().is_some()).unwrap_or(false);
		if has_ui_node {
			node_tree.with_component_mut::<UINode, _>(node_id, |ui_node, tree| {
				if ui_node.get_solver().is_none() {
					let _ = ui_node.set_solver(Some(solver.clone()));
				}
				if let Some(node) = tree.get_node(node_id) {
					let intrinsic_content_size = UINode::collect_intrinsic_content_size(node);
					let _ = ui_node.set_intrinsic_content_size(intrinsic_content_size);
				}
			});
		}
		let child_ids: Vec<NodeId> = node_tree.get_child_ids(node_id).to_vec();
		for child_id in child_ids {
			UINode::attach_pending_in_tree(node_tree, child_id, solver);
		}
	}
}

impl Default for UINode {
	fn default() -> UINode {
		UINode::new()
	}
}

#[crate::reflect]
impl Component for UINode {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"UINode"
	}

	//==============================================================================
	// 붙음. (자리는 솔버 값이 정하므로 피봇과 앵커를 왼쪽 위로)
	//==============================================================================
	fn attach(&mut self, transform: &mut NodeTransform) {
		transform.set_pivot(Pivot::TOP_LEFT);
		transform.set_anchor(Pivot::TOP_LEFT);
	}

	//==============================================================================
	// 갱신. (노드 컴포넌트의 intrinsic content size 가 바뀌었으면 제약을 다시 걸고, 솔버 값을 노드에 옮긴다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		let intrinsic_content_size = UINode::collect_intrinsic_content_size(context.get_node());
		let _ = self.set_intrinsic_content_size(intrinsic_content_size);
		let node_id = context.get_node_id();
		let parent_left_top = UINode::read_parent_left_top(context.get_node_tree(), node_id);
		self.apply_to_node(context.get_node_mut(), parent_left_top);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 화면 노드(0, 0, 800 x 600) 밑에 여백 20 의 자식, 그 밑에 가운데 놓인 100 x 40 손주. 노드 자리는 부모 왼쪽 위 기준이다.
	//==============================================================================
	#[test]
	fn constraints_place_nested_nodes() {
		let solver = LayoutSolver::new();
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let mut screen_node = UINode::new();
		screen_node.set_solver(Some(solver.clone())).expect("solver");
		screen_node.add_constraint(&screen_node.left_anchor().equal_to(0)).expect("left");
		screen_node.add_constraint(&screen_node.top_anchor().equal_to(0)).expect("top");
		screen_node.add_constraint(&screen_node.width_anchor().equal_to(800)).expect("width");
		screen_node.add_constraint(&screen_node.height_anchor().equal_to(600)).expect("height");

		let panel_id = node_tree.create_child(root_id, "panel");
		node_tree.add_component(panel_id, UINode::new());
		node_tree.with_component_mut::<UINode, _>(panel_id, |panel, _tree| {
			panel.set_solver(Some(solver.clone())).expect("solver");
			panel.add_constraint(&panel.left_anchor().equal_to(screen_node.left_anchor().add(20))).expect("left");
			panel.add_constraint(&panel.top_anchor().equal_to(screen_node.top_anchor().add(20))).expect("top");
			panel.add_constraint(&panel.right_anchor().equal_to(screen_node.right_anchor().subtract(20))).expect("right");
			panel.add_constraint(&panel.bottom_anchor().equal_to(screen_node.bottom_anchor().subtract(20))).expect("bottom");
		});
		let (panel_center_x, panel_center_y) = {
			let panel = node_tree[panel_id].get_component::<UINode>().expect("panel");
			(panel.center_x_anchor(), panel.center_y_anchor())
		};
		let button_id = node_tree.create_child(panel_id, "button");
		node_tree.add_component(button_id, UINode::new());
		node_tree.with_component_mut::<UINode, _>(button_id, |button, _tree| {
			button.set_solver(Some(solver.clone())).expect("solver");
			button.add_constraint(&button.width_anchor().equal_to(100)).expect("width");
			button.add_constraint(&button.height_anchor().equal_to(40)).expect("height");
			button.add_constraint(&button.center_x_anchor().equal_to(panel_center_x.clone())).expect("center x");
			button.add_constraint(&button.center_y_anchor().equal_to(panel_center_y.clone())).expect("center y");
		});
		solver.update_variables();
		UINode::apply_layout_in_tree(&mut node_tree, root_id);
		assert_eq!(node_tree[panel_id].get_local_position(), Vector2::create(20.0, 20.0));
		assert_eq!(node_tree[panel_id].get_content_size(), Vector2::create(760.0, 560.0));
		assert_eq!(node_tree[button_id].get_local_position(), Vector2::create(330.0, 260.0));
		assert_eq!(node_tree[button_id].get_content_size(), Vector2::create(100.0, 40.0));
		assert_eq!(node_tree[button_id].get_pivot(), Pivot::TOP_LEFT);
	}

	//==============================================================================
	// intrinsic size: hug 와 압축 저항이 너비를 자연 크기로 끈다. 우선순위를 바꾸면 다시 건다. 여백 가이드는 여백만큼 안쪽이다.
	//==============================================================================
	#[test]
	fn intrinsic_size_and_margins_guide() {
		let solver = LayoutSolver::new();
		let mut label = UINode::new();
		label.set_solver(Some(solver.clone())).expect("solver");
		label.add_constraint(&label.left_anchor().equal_to(10)).expect("left");
		label.add_constraint(&label.top_anchor().equal_to(5)).expect("top");
		label.set_intrinsic_content_size(Vector2::create(120.0, 30.0)).expect("intrinsic");
		solver.update_variables();
		assert!((label.get_width() - 120.0).abs() < 0.001);
		assert!((label.get_height() - 30.0).abs() < 0.001);
		label.add_constraint(&label.width_anchor().equal_to(200).with_strength(LayoutStrength::from_priority(500.0))).expect("width");
		solver.update_variables();
		assert!((label.get_width() - 200.0).abs() < 0.001);
		label.set_content_hugging_priority(600.0, LayoutConstraintAxis::Horizontal).expect("hugging");
		solver.update_variables();
		assert!((label.get_width() - 120.0).abs() < 0.001);
		assert_eq!(label.get_content_hugging_priority(LayoutConstraintAxis::Horizontal), 600.0);

		label.set_layout_margins(LayoutMargins::create(4.0, 6.0, 4.0, 6.0)).expect("margins");
		let guide_left = label.get_layout_margins_guide().get_left_variable().clone();
		let guide_right = label.get_layout_margins_guide().get_right_variable().clone();
		solver.update_variables();
		assert!((guide_left.get_value() - 16.0).abs() < 0.001);
		assert!((guide_right.get_value() - 124.0).abs() < 0.001);

		label.set_solver(None).expect("detach");
		assert!(!solver.has_constraint(&label.get_constraints()[0]));
	}
}
