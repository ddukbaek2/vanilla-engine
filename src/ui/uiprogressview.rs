//==============================================================================
//! 진행률 표시. (원본 ui/uiprogressview.js 의 UIProgressView, ProgressDirection)
//! - 호스트 content_size 영역을 트랙으로 쓰고, 값 비율만큼 채움을 그린다. 입력은 받지 않는다.
//! - 트랙 색은 원본처럼 UIView 의 배경색이다. (set_track_color 는 set_background_color 의 다른 이름)
//! - 반대편 값(secondary_value)을 정하면 진행 방향의 반대쪽 끝에서 차오르는 두 번째 채움을 그린다. (원본 null 은 None)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::rect::Rect;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::ui::uiview::UIView;

//==============================================================================
/// 진행 방향.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressDirection {
	/// 왼쪽에서 오른쪽으로 채움.
	Horizontal,
	/// 아래에서 위로 채움.
	Vertical,
}

// 채우는 방향의 반사 값. (원본 ProgressDirection 글)
crate::core::reflection::impl_reflect_names!(ProgressDirection, [
	(ProgressDirection::Horizontal, "horizontal"),
	(ProgressDirection::Vertical, "vertical"),
]);

//==============================================================================
/// 진행률 표시.
//==============================================================================
pub struct UIProgressView {
	ui_view: UIView,
	value: f32,
	min_value: f32,
	max_value: f32,
	fill_color: Color,
	secondary_value: Option<f32>,
	secondary_fill_color: Color,
	corner_radius: f32,
	direction: ProgressDirection,
}

#[crate::reflect(base = ui_view)]
impl UIProgressView {
	//==============================================================================
	/// 생성. (값 0, 범위 0 ~ 1, 트랙 회색, 채움 파랑)
	//==============================================================================
	pub fn new() -> UIProgressView {
		let mut ui_view = UIView::new();
		ui_view.set_background_color(Color::create(0.7, 0.7, 0.7, 1.0));
		UIProgressView {
			ui_view,
			value: 0.0,
			min_value: 0.0,
			max_value: 1.0,
			fill_color: Color::create(0.23, 0.51, 0.96, 1.0),
			secondary_value: None,
			secondary_fill_color: Color::create(0.94, 0.33, 0.31, 1.0),
			corner_radius: 0.0,
			direction: ProgressDirection::Horizontal,
		}
	}

	//==============================================================================
	/// 품은 뷰 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	/// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	/// 배경색 설정. (트랙 색)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	/// 배경색 반환. (트랙 색)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
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
	/// 0 ~ 1 비율 반환.
	//==============================================================================
	pub fn get_ratio(&self) -> f32 {
		let range = self.max_value - self.min_value;
		if range <= 0.0 {
			return 0.0;
		}
		let ratio = (self.value - self.min_value) / range;
		math::clamp(ratio, 0.0, 1.0)
	}

	//==============================================================================
	/// 값 설정. (범위로 자른다)
	//==============================================================================
	pub fn set_value(&mut self, value: f32) {
		self.value = math::clamp(value, self.min_value, self.max_value);
	}

	//==============================================================================
	/// 값 반환.
	//==============================================================================
	pub fn get_value(&self) -> f32 {
		self.value
	}

	//==============================================================================
	/// 범위 설정. (지금 값을 새 범위로 다시 자른다)
	//==============================================================================
	pub fn set_range(&mut self, min_value: f32, max_value: f32) {
		self.min_value = min_value;
		self.max_value = max_value;
		let value = self.value;
		self.set_value(value);
	}

	//==============================================================================
	/// 최소값 반환.
	//==============================================================================
	pub fn get_min_value(&self) -> f32 {
		self.min_value
	}

	//==============================================================================
	/// 최대값 반환.
	//==============================================================================
	pub fn get_max_value(&self) -> f32 {
		self.max_value
	}

	//==============================================================================
	/// 반대편 값 설정. (None 이면 끔. 범위로 자른다)
	//==============================================================================
	pub fn set_secondary_value(&mut self, secondary_value: Option<f32>) {
		self.secondary_value = secondary_value.map(|value| math::clamp(value, self.min_value, self.max_value));
	}

	//==============================================================================
	/// 반대편 값 반환.
	//==============================================================================
	pub fn get_secondary_value(&self) -> Option<f32> {
		self.secondary_value
	}

	//==============================================================================
	/// 반대편 채움 색 설정.
	//==============================================================================
	pub fn set_secondary_fill_color(&mut self, color: impl Into<Color>) {
		self.secondary_fill_color = color.into();
	}

	//==============================================================================
	/// 반대편 채움 색 반환.
	//==============================================================================
	pub fn get_secondary_fill_color(&self) -> Color {
		self.secondary_fill_color
	}

	//==============================================================================
	/// 트랙 색 설정. (배경색)
	//==============================================================================
	pub fn set_track_color(&mut self, color: impl Into<Color>) {
		self.ui_view.set_background_color(color);
	}

	//==============================================================================
	/// 트랙 색 반환. (배경색)
	//==============================================================================
	pub fn get_track_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	/// 채움 색 설정.
	//==============================================================================
	pub fn set_fill_color(&mut self, color: impl Into<Color>) {
		self.fill_color = color.into();
	}

	//==============================================================================
	/// 채움 색 반환.
	//==============================================================================
	pub fn get_fill_color(&self) -> Color {
		self.fill_color
	}

	//==============================================================================
	/// 둥근 모서리 반지름 설정.
	//==============================================================================
	pub fn set_corner_radius(&mut self, corner_radius: f32) {
		self.corner_radius = corner_radius;
	}

	//==============================================================================
	/// 둥근 모서리 반지름 반환.
	//==============================================================================
	pub fn get_corner_radius(&self) -> f32 {
		self.corner_radius
	}

	//==============================================================================
	/// 방향 설정.
	//==============================================================================
	pub fn set_direction(&mut self, direction: ProgressDirection) {
		self.direction = direction;
	}

	//==============================================================================
	/// 방향 반환.
	//==============================================================================
	pub fn get_direction(&self) -> ProgressDirection {
		self.direction
	}
}

impl Default for UIProgressView {
	fn default() -> UIProgressView {
		UIProgressView::new()
	}
}

#[crate::reflect]
impl Component for UIProgressView {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"UIProgressView"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (트랙 → 채움 → 반대편 채움. 둥근 모서리 때문에 배경을 직접 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}

		// 트랙.
		let track_rect = Rect::create(0.0, 0.0, content_size.x, content_size.y);
		let track_color = self.get_background_color();
		self.fill_rect(graphic, track_rect, track_color);

		// 채움.
		let ratio = self.get_ratio();
		if ratio <= 0.0 {
			return;
		}
		let fill_rect = match self.direction {
			ProgressDirection::Vertical => {
				let fill_height = content_size.y * ratio;
				Rect::create(0.0, content_size.y - fill_height, content_size.x, fill_height)
			}
			ProgressDirection::Horizontal => {
				let fill_width = content_size.x * ratio;
				Rect::create(0.0, 0.0, fill_width, content_size.y)
			}
		};
		let fill_color = self.fill_color;
		self.fill_rect(graphic, fill_rect, fill_color);

		// 반대편 게이지. (점유율 / 대전 게이지처럼 한 바에 양쪽을 함께 보일 때)
		let secondary_value = match self.secondary_value {
			Some(secondary_value) => {
				secondary_value
			}
			None => {
				return;
			}
		};
		let range = self.max_value - self.min_value;
		if range <= 0.0 {
			return;
		}
		let secondary_ratio = math::clamp((secondary_value - self.min_value) / range, 0.0, 1.0);
		if secondary_ratio <= 0.0 {
			return;
		}
		let secondary_rect = match self.direction {
			ProgressDirection::Vertical => {
				let secondary_height = content_size.y * secondary_ratio;
				Rect::create(0.0, 0.0, content_size.x, secondary_height)
			}
			ProgressDirection::Horizontal => {
				let secondary_width = content_size.x * secondary_ratio;
				Rect::create(content_size.x - secondary_width, 0.0, secondary_width, content_size.y)
			}
		};
		let secondary_fill_color = self.secondary_fill_color;
		self.fill_rect(graphic, secondary_rect, secondary_fill_color);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 값은 범위로 잘리고, 비율은 범위 안의 자리다.
	//==============================================================================
	#[test]
	fn value_clamp_and_ratio() {
		let mut progress_view = UIProgressView::new();
		progress_view.set_range(10.0, 20.0);
		assert_eq!(progress_view.get_value(), 10.0);
		progress_view.set_value(15.0);
		assert_eq!(progress_view.get_ratio(), 0.5);
		progress_view.set_value(99.0);
		assert_eq!(progress_view.get_value(), 20.0);
		progress_view.set_secondary_value(Some(-5.0));
		assert_eq!(progress_view.get_secondary_value(), Some(10.0));
		progress_view.set_range(0.0, 0.0);
		assert_eq!(progress_view.get_ratio(), 0.0);
		progress_view.set_track_color(Color::black());
		assert_eq!(progress_view.get_background_color(), Color::black());
	}
}
