//==============================================================================
//! 2D 물리 그리기 도구. (확인용 선과 면)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. 원본 src/experimental/physics2d/physicsdebugdraw2d.js 와 같은 API, 같은 색이다.
//! - 모양을 몸 상태 색으로 칠하고(정적 초록, 운동학 파랑, 동적 주황, 잠든 몸 회색, 센서 하늘색), 원은 돌림을 보이는 반지름 선을, 관절은 두 닻을 잇는 선을 긋는다.
//! - 월드 좌표를 그대로 그리므로 그래픽의 좌표계가 월드와 같아야 한다. (화소 단위 월드면 씬의 draw 에서 바로 부른다)
//==============================================================================
use super::body2d::BodyType2D;
use super::geometry2d::ShapeType2D;
use super::narrowphase2d::{fill_world_shape, WorldShape};
use super::physicsworld2d::PhysicsWorld2D;
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;

/// 둥근 모서리 하나를 나누는 조각 수.
const CORNER_SEGMENT_COUNT: usize = 6;
/// 정적 몸 색.
const STATIC_COLOR: Color = Color::create(0.50, 0.75, 0.50, 1.0);
/// 운동학 몸 색.
const KINEMATIC_COLOR: Color = Color::create(0.50, 0.62, 0.90, 1.0);
/// 깨어 있는 동적 몸 색.
const DYNAMIC_COLOR: Color = Color::create(0.90, 0.70, 0.30, 1.0);
/// 잠든 동적 몸 색.
const SLEEPING_COLOR: Color = Color::create(0.55, 0.55, 0.60, 1.0);
/// 센서 색.
const SENSOR_COLOR: Color = Color::create(0.30, 0.80, 0.90, 1.0);
/// 관절 색.
const JOINT_COLOR: Color = Color::create(0.85, 0.85, 0.85, 1.0);
/// 채우기는 선 색을 이만큼 옅게.
const FILL_ALPHA: f64 = 0.45;

//==============================================================================
/// 2D 물리 그리기 도구. (정적 함수 묶음)
///
/// # 예제
/// 씬의 draw 에서 월드 그리기
/// ```rust
/// use vanilla::Graphic;
/// use vanilla::experimental::physics2d::{PhysicsDebugDraw2D, PhysicsWorld2D};
///
/// fn draw_physics(world: &PhysicsWorld2D, graphic: &mut Graphic) {
/// 	PhysicsDebugDraw2D::draw(world, graphic, 2.0);
/// }
/// ```
//==============================================================================
pub struct PhysicsDebugDraw2D;

impl PhysicsDebugDraw2D {
	//==============================================================================
	/// 월드 그리기.
	///
	/// # 인자
	/// - `world`: 그릴 월드.
	/// - `graphic`: 그릴 그래픽.
	/// - `line_width`: 선 굵기. (그래픽 좌표 단위)
	//==============================================================================
	pub fn draw(world: &PhysicsWorld2D, graphic: &mut Graphic, line_width: f64) {
		let mut world_shape = WorldShape::new();
		for body_data in world.body_list.iter().flatten() {
			let body_color = if body_data.body_type == BodyType2D::Static {
				STATIC_COLOR
			}
			else if body_data.body_type == BodyType2D::Kinematic {
				KINEMATIC_COLOR
			}
			else if !body_data.is_awake {
				SLEEPING_COLOR
			}
			else {
				DYNAMIC_COLOR
			};
			for shape_id in body_data.shape_id_list.iter() {
				let Some(shape_data) = world.shape_list[shape_id.get_index()].as_ref() else {
					continue;
				};
				let line_color = if shape_data.is_sensor {
					SENSOR_COLOR
				}
				else {
					body_color
				};
				let fill_color = Color::create(line_color.red, line_color.green, line_color.blue, FILL_ALPHA);
				fill_world_shape(shape_data, body_data.position_x, body_data.position_y, body_data.rotation_cosine, body_data.rotation_sine, &mut world_shape);
				PhysicsDebugDraw2D::draw_world_shape(graphic, &world_shape, body_data.rotation_cosine, body_data.rotation_sine, fill_color, line_color, line_width);
			}
		}
		graphic.set_stroke_color(JOINT_COLOR);
		for joint_data in world.joint_list.iter().flatten() {
			let (Some(body_a), Some(body_b)) = (world.body_list[joint_data.body_index_a].as_ref(), world.body_list[joint_data.body_index_b].as_ref()) else {
				continue;
			};
			let anchor_a = Vector2::create(body_a.rotation_cosine * joint_data.local_anchor_a_x - body_a.rotation_sine * joint_data.local_anchor_a_y + body_a.position_x, body_a.rotation_sine * joint_data.local_anchor_a_x + body_a.rotation_cosine * joint_data.local_anchor_a_y + body_a.position_y);
			let anchor_b = Vector2::create(body_b.rotation_cosine * joint_data.local_anchor_b_x - body_b.rotation_sine * joint_data.local_anchor_b_y + body_b.position_x, body_b.rotation_sine * joint_data.local_anchor_b_x + body_b.rotation_cosine * joint_data.local_anchor_b_y + body_b.position_y);
			graphic.draw_line(&[anchor_a, anchor_b], line_width);
			graphic.set_fill_color(JOINT_COLOR);
			graphic.draw_circle(anchor_a, line_width * 1.5);
			graphic.draw_circle(anchor_b, line_width * 1.5);
		}
	}

	//==============================================================================
	/// 월드 모양 하나 그리기.
	//==============================================================================
	fn draw_world_shape(graphic: &mut Graphic, world_shape: &WorldShape, rotation_cosine: f64, rotation_sine: f64, fill_color: Color, line_color: Color, line_width: f64) {
		graphic.set_stroke_color(line_color);
		if world_shape.shape_type == ShapeType2D::Circle {
			let center = Vector2::create(world_shape.center_x, world_shape.center_y);
			let radius = world_shape.radius;
			graphic.set_fill_color(fill_color);
			graphic.draw_circle(center, radius);
			graphic.draw_stroke_circle(center, radius, line_width);
			let edge = Vector2::create(world_shape.center_x + rotation_cosine * world_shape.radius, world_shape.center_y + rotation_sine * world_shape.radius);
			graphic.draw_line(&[center, edge], line_width);
			return;
		}
		if world_shape.shape_type == ShapeType2D::Segment {
			let point1 = Vector2::create(world_shape.vertex_x[0], world_shape.vertex_y[0]);
			let point2 = Vector2::create(world_shape.vertex_x[1], world_shape.vertex_y[1]);
			graphic.draw_line(&[point1, point2], line_width);
			return;
		}
		let mut positions = Vec::new();
		if world_shape.radius <= 0.0 {
			for index in 0..world_shape.count {
				positions.push(Vector2::create(world_shape.vertex_x[index], world_shape.vertex_y[index]));
			}
		}
		else {
			// 둥근 모서리. (꼭짓점마다 앞 변 법선에서 다음 변 법선까지 호)
			for index in 0..world_shape.count {
				let previous = (index + world_shape.count - 1) % world_shape.count;
				let start_angle = world_shape.normal_y[previous].atan2(world_shape.normal_x[previous]);
				let mut sweep_angle = world_shape.normal_y[index].atan2(world_shape.normal_x[index]) - start_angle;
				while sweep_angle < 0.0 {
					sweep_angle += 2.0 * std::f64::consts::PI;
				}
				for segment_index in 0..=CORNER_SEGMENT_COUNT {
					let angle = start_angle + sweep_angle * segment_index as f64 / CORNER_SEGMENT_COUNT as f64;
					positions.push(Vector2::create(world_shape.vertex_x[index] + world_shape.radius * angle.cos(), world_shape.vertex_y[index] + world_shape.radius * angle.sin()));
				}
			}
		}
		graphic.draw_convex_polygon(&positions, fill_color);
		graphic.draw_line_loop(&positions, line_width);
	}
}
