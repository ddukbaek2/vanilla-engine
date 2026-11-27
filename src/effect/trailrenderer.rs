//==============================================================================
//! 트레일 렌더러 컴포넌트.
//! - 원본 effect/trailrenderer.js 를 옮긴 것이다. 노드가 지나간 자리를 점으로 기록해 굵기 / 색이 잦아드는 띠로 그린다. (유니티 TrailRenderer 의 2D 대응)
//! - 점은 노드의 로컬 위치(부모 좌표)로 기록하고, 그릴 때는 노드 원점 기준 상대 좌표로 바꿔 노드 트랜스폼 아래에서 그린다. (원본과 같다)
//! - 세그먼트마다 draw_line 한 번이라 묶어 그리기로 한 드로우 콜에 모인다. (더하기 합성이면 블렌드가 바뀌는 앞뒤로 끊긴다)
//! - 사용:
//!   ```text
//!     let trail = node.add_component(TrailRenderer::new());
//!     trail.set_time(0.5);
//!     trail.set_width(14.0, 0.0);
//!     trail.set_colors(Color::create(0.4, 0.8, 1.0, 0.9), Color::create(0.4, 0.8, 1.0, 0.0));
//!   ```
//! - 원본과 다른 점: 원본은 색을 "rgba(r, g, b, a)" 문자열로 넘겨 RGB 가 0 ~ 255 정수로 반올림된다. 러스트 판도 같은 반올림을 해서 Color 로 넘긴다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;

//==============================================================================
/// 기록한 점.
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct TrailPoint {
	x: f64,
	y: f64,
	age: f64,
}

//==============================================================================
/// 트레일 렌더러 컴포넌트.
/// - tick 마다 노드 로컬 위치를 점으로 찍고, 수명이 지난 점은 지운다. 점이 둘 이상이면 새 점 쪽에서 오래된 점 쪽으로 굵기와 색이 잦아드는 띠를 그린다.
///
/// # 예제
/// 혜성 노드를 옮기며 점 쌓기
/// ```rust
/// use vanilla::{Color, Engine, EngineConfiguration, NodeTree, TrailRenderer, Vector2};
///
/// let engine = Engine::new(EngineConfiguration::new());
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let comet_id = node_tree.create_child(root_id, "Comet");
/// let trail = node_tree[comet_id].add_component(TrailRenderer::new());
/// trail.set_time(0.5);
/// trail.set_width(14.0, 0.0);
/// trail.set_colors(Color::create(0.4, 0.8, 1.0, 0.9), Color::create(0.4, 0.8, 1.0, 0.0));
/// for step in 0..5 {
/// 	node_tree[comet_id].set_local_position(Vector2::create(step as f64 * 20.0, 0.0));
/// 	node_tree.tick(&engine, 0.1);
/// }
/// assert_eq!(node_tree[comet_id].get_component::<TrailRenderer>().map(|trail| trail.get_point_count()), Some(5));
/// ```
/// <!-- image: trail.webp -->
//==============================================================================
pub struct TrailRenderer {
	component_base: ComponentBase,
	point_list: Vec<TrailPoint>,
	/// 점이 남는 시간. (초)
	point_lifetime: f64,
	/// 이보다 가까우면 점을 추가하지 않는다.
	min_vertex_distance: f64,
	start_width: f64,
	end_width: f64,
	start_color: Color,
	end_color: Color,
	is_additive: bool,
	is_emitting: bool,
}

#[crate::reflect]
impl TrailRenderer {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 점이 없고 수명 0.5 초, 최소 거리 3, 굵기 10 → 0, 흰색 알파 0.9 → 0, 보통 합성, 점 찍기를 켠 트레일.
	//==============================================================================
	pub fn new() -> TrailRenderer {
		TrailRenderer {
			component_base: ComponentBase::new(),
			point_list: Vec::new(),
			point_lifetime: 0.5,
			min_vertex_distance: 3.0,
			start_width: 10.0,
			end_width: 0.0,
			start_color: Color::create(1.0, 1.0, 1.0, 0.9),
			end_color: Color::create(1.0, 1.0, 1.0, 0.0),
			is_additive: false,
			is_emitting: true,
		}
	}

	//==============================================================================
	/// 흔적 지우기.
	//==============================================================================
	pub fn clear(&mut self) {
		self.component_base.mark_drawn_rect_dirty();
		self.point_list.clear();
	}

	//==============================================================================
	/// 점이 남는 시간 설정. (초)
	///
	/// # 인자
	/// - `point_lifetime`: 점이 남는 시간. (초, 기본 0.5) 나이가 이만큼 된 점은 tick 에서 지운다.
	//==============================================================================
	pub fn set_time(&mut self, point_lifetime: f64) {
		self.point_lifetime = point_lifetime;
	}

	//==============================================================================
	/// 점 사이 최소 거리 설정.
	///
	/// # 인자
	/// - `min_vertex_distance`: 새 점을 찍을 가장 짧은 거리. (부모 좌표, 기본 3) 마지막 점에서 이보다 덜 움직였으면 점을 찍지 않는다.
	//==============================================================================
	pub fn set_min_vertex_distance(&mut self, min_vertex_distance: f64) {
		self.min_vertex_distance = min_vertex_distance;
	}

	//==============================================================================
	/// 굵기 설정. (새 점 쪽, 오래된 점 쪽)
	///
	/// # 인자
	/// - `start_width`: 새 점 쪽 굵기. (노드 좌표, 기본 10)
	/// - `end_width`: 오래된 점 쪽 굵기. (노드 좌표, 기본 0. 그릴 때 0.1 보다 가늘게 하지 않는다)
	//==============================================================================
	pub fn set_width(&mut self, start_width: f64, end_width: f64) {
		self.component_base.mark_drawn_rect_dirty();
		self.start_width = start_width;
		self.end_width = end_width;
	}

	//==============================================================================
	/// 색 설정. (새 점 쪽, 오래된 점 쪽)
	/// - 그릴 때 RGB 는 원본처럼 0 ~ 255 정수로 반올림하고 알파는 그대로 보간한다.
	///
	/// # 인자
	/// - `start_color`: 새 점 쪽 색. (Color 나 "#rrggbb" 같은 글, 기본 흰색 알파 0.9)
	/// - `end_color`: 오래된 점 쪽 색. (기본 흰색 알파 0)
	//==============================================================================
	pub fn set_colors(&mut self, start_color: impl Into<Color>, end_color: impl Into<Color>) {
		self.start_color = start_color.into();
		self.end_color = end_color.into();
	}

	//==============================================================================
	/// 더하기 합성 여부 설정. (켜면 lighter)
	///
	/// # 인자
	/// - `is_additive`: 참이면 띠를 더하기 합성으로 그리고 끝나면 보통 합성으로 되돌린다. (기본 거짓)
	//==============================================================================
	pub fn set_additive(&mut self, is_additive: bool) {
		self.is_additive = is_additive;
	}

	//==============================================================================
	/// 점 만들기 여부 설정. (거짓이면 새 점을 만들지 않는다. 남은 띠는 잦아든다)
	///
	/// # 인자
	/// - `is_emitting`: 참이면 tick 마다 점을 찍는다. (기본 참)
	//==============================================================================
	pub fn set_emitting(&mut self, is_emitting: bool) {
		self.is_emitting = is_emitting;
	}

	//==============================================================================
	/// 남아 있는 점 수 반환.
	///
	/// # 돌려줌
	/// 수명이 남은 점 수. (둘 이상이어야 띠가 그려진다)
	//==============================================================================
	pub fn get_point_count(&self) -> usize {
		self.point_list.len()
	}
}

impl Default for TrailRenderer {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> TrailRenderer {
		TrailRenderer::new()
	}
}

#[crate::reflect]
impl Component for TrailRenderer {
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
	/// 컴포넌트 종류 이름 반환. ("TrailRenderer")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"TrailRenderer"
	}

	//==============================================================================
	/// 갱신. (점 기록 + 노화)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f64) {
		let local_position = context.get_transform().get_local_position();

		// 점이 있으면 나이 들고 빠지며 노드 자리도 바뀌므로 그리는 사각형을 다시 셈하게 한다. (하위 트리 그리기 경계)
		if !self.point_list.is_empty() {
			self.component_base.mark_drawn_rect_dirty();
		}
		let point_lifetime = self.point_lifetime;
		for point in self.point_list.iter_mut() {
			point.age += time_delta;
		}
		self.point_list.retain(|point| point.age < point_lifetime);

		if !self.is_emitting {
			return;
		}
		if let Some(last_point) = self.point_list.last() {
			let delta_x = local_position.x - last_point.x;
			let delta_y = local_position.y - last_point.y;
			if delta_x * delta_x + delta_y * delta_y < self.min_vertex_distance * self.min_vertex_distance {
				return;
			}
		}
		self.component_base.mark_drawn_rect_dirty();
		self.point_list.push(TrailPoint {
			x: local_position.x,
			y: local_position.y,
			age: 0.0,
		});
	}

	//==============================================================================
	/// 그리는 사각형. (원본 TrailRenderer.calculateDrawnRect)
	/// - 점들의 경계 상자를 지금 노드 자리만큼 당기고 큰 굵기의 반 + 1 만큼 넓힌 것. 점이 둘보다 적으면 크기 0.
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, _graphic: &mut Graphic) -> Option<Rect> {
		if self.point_list.len() < 2 {
			return Some(Rect::create(0.0, 0.0, 0.0, 0.0));
		}
		let node_position = transform.get_local_position();
		let extent = self.start_width.abs().max(self.end_width.abs()).max(0.1) * 0.5 + 1.0;
		let mut minimum_x = f64::INFINITY;
		let mut minimum_y = f64::INFINITY;
		let mut maximum_x = f64::NEG_INFINITY;
		let mut maximum_y = f64::NEG_INFINITY;
		for point in self.point_list.iter() {
			minimum_x = minimum_x.min(point.x);
			minimum_y = minimum_y.min(point.y);
			maximum_x = maximum_x.max(point.x);
			maximum_y = maximum_y.max(point.y);
		}
		Some(Rect::create(minimum_x - node_position.x - extent, minimum_y - node_position.y - extent, maximum_x - minimum_x + extent * 2.0, maximum_y - minimum_y + extent * 2.0))
	}

	//==============================================================================
	/// 출력. (세그먼트마다 굵기 / 색 보간, 노드 원점 기준 상대 좌표로 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.point_list.len() < 2 {
			return;
		}
		if self.is_additive {
			graphic.set_blend_mode(BlendMode::Lighter);
		}
		let node_position = transform.get_local_position();
		let start_color = self.start_color;
		let end_color = self.end_color;

		for point_index in 0..self.point_list.len() - 1 {
			let point = self.point_list[point_index];
			let next_point = self.point_list[point_index + 1];
			let age_ratio = point.age / self.point_lifetime;
			let fresh_ratio = 1.0 - age_ratio;
			let segment_width = (self.end_width + (self.start_width - self.end_width) * fresh_ratio).max(0.1);
			let red = ((end_color.red + (start_color.red - end_color.red) * fresh_ratio) * 255.0).round() / 255.0;
			let green = ((end_color.green + (start_color.green - end_color.green) * fresh_ratio) * 255.0).round() / 255.0;
			let blue = ((end_color.blue + (start_color.blue - end_color.blue) * fresh_ratio) * 255.0).round() / 255.0;
			let alpha = end_color.alpha + (start_color.alpha - end_color.alpha) * fresh_ratio;
			graphic.set_stroke_color(Color::create(red, green, blue, alpha));
			graphic.draw_line(&[
				Vector2::create(point.x - node_position.x, point.y - node_position.y),
				Vector2::create(next_point.x - node_position.x, next_point.y - node_position.y),
			], segment_width);
		}

		if self.is_additive {
			graphic.set_blend_mode(BlendMode::Normal);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::node::nodetree::NodeTree;

	//==============================================================================
	/// 움직인 만큼 점이 쌓이고, 가까우면 건너뛰고, 수명이 지나면 사라진다.
	//==============================================================================
	#[test]
	fn points_follow_node_and_expire() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let trail_id = node_tree.create_child(root_id, "trail");
		let trail = node_tree[trail_id].add_component(TrailRenderer::new());
		trail.set_time(0.5);
		trail.set_min_vertex_distance(3.0);

		node_tree.tick(&engine, 0.1);
		node_tree[trail_id].set_local_position(Vector2::create(1.0, 0.0));
		node_tree.tick(&engine, 0.1);
		node_tree[trail_id].set_local_position(Vector2::create(10.0, 0.0));
		node_tree.tick(&engine, 0.1);
		assert_eq!(node_tree[trail_id].get_component::<TrailRenderer>().map(|trail| trail.get_point_count()), Some(2));

		if let Some(trail) = node_tree[trail_id].get_component_mut::<TrailRenderer>() {
			trail.set_emitting(false);
		}
		node_tree.tick(&engine, 0.6);
		assert_eq!(node_tree[trail_id].get_component::<TrailRenderer>().map(|trail| trail.get_point_count()), Some(0));
	}
}
