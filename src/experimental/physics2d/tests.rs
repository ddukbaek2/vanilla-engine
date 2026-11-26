//==============================================================================
//! 2D 물리 시험.
//! - comparison_matches_vanilla_js: 원본 vanilla.js 의 tools/physics2dcompare.mjs 와 같은 장면을 돌려, 원본이 쓴 덤프(comparison.txt)와 비트까지 견준다.
//!   (덤프를 다시 쓰려면 원본 저장소에서 node tools/physics2dcompare.mjs --write <이 폴더>/comparison.txt)
//==============================================================================
use super::*;
use crate::experimental::spatial::aabb2d::AABB2D;
use std::f64::consts::PI;

//==============================================================================
/// f64 를 16 진수 비트로.
//==============================================================================
fn to_bits(value: f64) -> String {
	format!("{:016x}", value.to_bits())
}

//==============================================================================
/// 동적 몸 하나 만들기.
//==============================================================================
fn create_dynamic_body(world: &mut PhysicsWorld2D, x: f64, y: f64, angle: f64) -> BodyId2D {
	let mut body_definition = BodyDefinition2D::new();
	body_definition.body_type = BodyType2D::Dynamic;
	body_definition.position = PhysicsVector2::create(x, y);
	body_definition.angle = angle;
	world.create_body(&body_definition)
}

//==============================================================================
/// 원본 tools/physics2dcompare.mjs 와 같은 장면을 돌려 덤프 글을 돌려준다.
//==============================================================================
fn run_comparison_scene() -> String {
	let mut line_list: Vec<String> = Vec::new();
	let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
	let shape_definition = ShapeDefinition2D::new();

	// 땅.
	let ground_id = world.create_body(&BodyDefinition2D::new());
	world.create_shape(ground_id, &shape_definition, Segment2D::create(PhysicsVector2::create(-30.0, 0.0), PhysicsVector2::create(30.0, 0.0)));
	let mut chain_definition = ChainDefinition2D::new();
	chain_definition.points = vec![PhysicsVector2::create(-30.0, -6.0), PhysicsVector2::create(-26.0, -1.0), PhysicsVector2::create(-22.0, 0.0)];
	world.create_chain(ground_id, &chain_definition);
	world.create_shape(ground_id, &shape_definition, Polygon2D::make_offset_box(1.0, 0.25, PhysicsVector2::create(12.0, -2.0), 0.3));

	// 피라미드.
	let row_count = 6;
	for row in 0..row_count {
		for column in 0..row_count - row {
			let x = (column as f64 - (row_count - row) as f64 * 0.5) * 0.9 + 0.45;
			let y = -0.4 - row as f64 * 0.9;
			let body_id = create_dynamic_body(&mut world, x, y, 0.0);
			world.create_shape(body_id, &shape_definition, Polygon2D::make_box(0.4, 0.4));
		}
	}

	// 튀는 공.
	let mut ball_definition = ShapeDefinition2D::new();
	ball_definition.restitution = 0.4;
	ball_definition.friction = 0.3;
	for index in 0..5 {
		let body_id = create_dynamic_body(&mut world, 6.0 + index as f64 * 0.7, -3.0 - index as f64 * 1.3, 0.0);
		world.create_shape(body_id, &ball_definition, Circle2D::create(PhysicsVector2::zero(), 0.3));
	}

	// 캡슐과 둥근 육각형.
	for index in 0..3 {
		let body_id = create_dynamic_body(&mut world, -6.0 + index as f64 * 1.5, -2.0 - index as f64, 0.2 * index as f64);
		world.create_shape(body_id, &shape_definition, Capsule2D::create(PhysicsVector2::create(-0.5, 0.0), PhysicsVector2::create(0.5, 0.0), 0.2));
	}
	let mut hexagon_point_list = Vec::new();
	for index in 0..6 {
		let rotation = PhysicsMath2D::compute_rotation(index as f64 * PI / 3.0);
		hexagon_point_list.push(PhysicsVector2::create(0.5 * rotation.cosine, 0.5 * rotation.sine));
	}
	let hexagon = Polygon2D::make_polygon(&hexagon_point_list, 0.05).expect("육각형");
	let hexagon_id = create_dynamic_body(&mut world, -9.0, -4.0, 0.1);
	world.create_shape(hexagon_id, &shape_definition, hexagon);

	// 회전 관절 사슬.
	let anchor_id = world.create_body(&BodyDefinition2D::new());
	let mut previous_id = anchor_id;
	for index in 0..6 {
		let link_id = create_dynamic_body(&mut world, -14.0 + index as f64 * 0.8 + 0.4, -8.0, 0.0);
		world.create_shape(link_id, &shape_definition, Capsule2D::create(PhysicsVector2::create(-0.4, 0.0), PhysicsVector2::create(0.4, 0.0), 0.1));
		let mut joint_definition = RevoluteJointDefinition2D::new();
		joint_definition.body_id_a = Some(previous_id);
		joint_definition.body_id_b = Some(link_id);
		joint_definition.local_anchor_a = if previous_id == anchor_id {
			PhysicsVector2::create(-14.0, -8.0)
		}
		else {
			PhysicsVector2::create(0.4, 0.0)
		};
		joint_definition.local_anchor_b = PhysicsVector2::create(-0.4, 0.0);
		if index == 0 {
			joint_definition.enable_limit = true;
			joint_definition.lower_angle = -0.5;
			joint_definition.upper_angle = 1.2;
		}
		world.create_revolute_joint(&joint_definition);
		previous_id = link_id;
	}

	// 수레.
	let chassis_id = create_dynamic_body(&mut world, 16.0, -1.2, 0.0);
	world.create_shape(chassis_id, &shape_definition, Polygon2D::make_box(1.0, 0.25));
	for index in 0..2 {
		let offset_x = if index == 0 {
			-0.7
		}
		else {
			0.7
		};
		let wheel_id = create_dynamic_body(&mut world, 16.0 + offset_x, -0.5, 0.0);
		world.create_shape(wheel_id, &shape_definition, Circle2D::create(PhysicsVector2::zero(), 0.35));
		let mut wheel_definition = WheelJointDefinition2D::new();
		wheel_definition.body_id_a = Some(chassis_id);
		wheel_definition.body_id_b = Some(wheel_id);
		wheel_definition.local_anchor_a = PhysicsVector2::create(offset_x, 0.7);
		wheel_definition.hertz = 5.0;
		wheel_definition.damping_ratio = 0.7;
		wheel_definition.enable_motor = true;
		wheel_definition.motor_speed = -4.0;
		wheel_definition.max_motor_torque = 10.0;
		wheel_definition.enable_limit = true;
		wheel_definition.lower_translation = -0.3;
		wheel_definition.upper_translation = 0.3;
		world.create_wheel_joint(&wheel_definition);
	}

	// 거리 용수철과 용접.
	let bob_id = create_dynamic_body(&mut world, -18.0, -6.0, 0.0);
	world.create_shape(bob_id, &shape_definition, Circle2D::create(PhysicsVector2::zero(), 0.25));
	let mut distance_definition = DistanceJointDefinition2D::new();
	distance_definition.body_id_a = Some(anchor_id);
	distance_definition.body_id_b = Some(bob_id);
	distance_definition.local_anchor_a = PhysicsVector2::create(-18.0, -9.0);
	distance_definition.length = 2.0;
	distance_definition.enable_spring = true;
	distance_definition.hertz = 2.0;
	distance_definition.damping_ratio = 0.2;
	distance_definition.enable_limit = true;
	distance_definition.min_length = 1.0;
	distance_definition.max_length = 3.0;
	world.create_distance_joint(&distance_definition);
	let rope_bob_id = create_dynamic_body(&mut world, -20.0, -7.0, 0.0);
	world.create_shape(rope_bob_id, &shape_definition, Polygon2D::make_box(0.2, 0.2));
	let mut rope_definition = DistanceJointDefinition2D::new();
	rope_definition.body_id_a = Some(anchor_id);
	rope_definition.body_id_b = Some(rope_bob_id);
	rope_definition.local_anchor_a = PhysicsVector2::create(-21.0, -9.0);
	rope_definition.length = 2.5;
	world.create_distance_joint(&rope_definition);
	let weld_a_id = create_dynamic_body(&mut world, 3.0, -6.0, 0.0);
	world.create_shape(weld_a_id, &shape_definition, Polygon2D::make_box(0.5, 0.2));
	let weld_b_id = create_dynamic_body(&mut world, 4.0, -6.0, 0.0);
	world.create_shape(weld_b_id, &shape_definition, Polygon2D::make_box(0.5, 0.2));
	let mut weld_definition = WeldJointDefinition2D::new();
	weld_definition.body_id_a = Some(weld_a_id);
	weld_definition.body_id_b = Some(weld_b_id);
	weld_definition.local_anchor_a = PhysicsVector2::create(0.5, 0.0);
	weld_definition.local_anchor_b = PhysicsVector2::create(-0.5, 0.0);
	weld_definition.angular_hertz = 3.0;
	weld_definition.angular_damping_ratio = 0.5;
	world.create_weld_joint(&weld_definition);

	// 센서와 운동학 발판.
	let sensor_body_id = world.create_body(&BodyDefinition2D::new());
	let mut sensor_definition = ShapeDefinition2D::new();
	sensor_definition.is_sensor = true;
	world.create_shape(sensor_body_id, &sensor_definition, Polygon2D::make_offset_box(2.0, 1.0, PhysicsVector2::create(7.5, -1.5), 0.0));
	let mut platform_definition = BodyDefinition2D::new();
	platform_definition.body_type = BodyType2D::Kinematic;
	platform_definition.position = PhysicsVector2::create(-3.0, -5.0);
	platform_definition.linear_velocity = PhysicsVector2::create(1.0, 0.0);
	let platform_id = world.create_body(&platform_definition);
	world.create_shape(platform_id, &shape_definition, Polygon2D::make_box(1.5, 0.2));
	let rider_id = create_dynamic_body(&mut world, -3.0, -5.6, 0.0);
	world.create_shape(rider_id, &shape_definition, Polygon2D::make_box(0.3, 0.3));

	// 걸음.
	let query_filter = QueryFilter2D::new();
	for step_index in 0..600 {
		if step_index == 30 {
			let body = &mut world[rider_id];
			let point = body.get_world_point(PhysicsVector2::create(0.1, 0.0));
			body.apply_linear_impulse(PhysicsVector2::create(0.5, -1.0), point, true);
		}
		if step_index % 120 == 60 {
			let platform = &mut world[platform_id];
			let velocity = platform.get_linear_velocity();
			platform.set_linear_velocity(PhysicsVector2::create(-velocity.x, 0.0));
		}
		if step_index == 200 {
			world[bob_id].set_transform(PhysicsVector2::create(-17.0, -7.0), 0.5);
		}
		world.step(1.0 / 60.0, 4);
		let begin_count = world.get_contact_begin_events().len();
		let end_count = world.get_contact_end_events().len();
		let sensor_begin_count = world.get_sensor_begin_events().len();
		let sensor_end_count = world.get_sensor_end_events().len();
		if begin_count + end_count + sensor_begin_count + sensor_end_count > 0 {
			line_list.push(format!("step {} events {} {} {} {}", step_index, begin_count, end_count, sensor_begin_count, sensor_end_count));
		}
		if step_index % 100 == 99 {
			match world.cast_ray_closest(PhysicsVector2::create(-30.0, -0.2), PhysicsVector2::create(60.0, 0.0), &query_filter) {
				None => line_list.push(format!("step {} ray none", step_index)),
				Some(hit) => line_list.push(format!("step {} ray {} {} {} {} {}", step_index, hit.shape_id.get_index(), to_bits(hit.fraction), to_bits(hit.point.x), to_bits(hit.normal.x), to_bits(hit.normal.y))),
			}
			line_list.push(format!("step {} awake {} contacts {}", step_index, world.get_awake_body_count(), world.get_contact_count()));
			// 질의: 모든 광선 맞음(돌려준 값 1 로 끝까지), 상자, 점, 모양 겹침. (부른 차례대로 모양 번호)
			let mut ray_hit_list: Vec<String> = Vec::new();
			world.cast_ray(PhysicsVector2::create(-8.0, -3.0), PhysicsVector2::create(20.0, 2.5), &query_filter, |shape_id, point, normal, fraction| {
				ray_hit_list.push(format!("{}:{}:{}:{}", shape_id.get_index(), to_bits(fraction), to_bits(point.y), to_bits(normal.x)));
				1.0
			});
			line_list.push(format!("step {} rays {}", step_index, ray_hit_list.join(" ")));
			let mut aabb_list: Vec<String> = Vec::new();
			world.overlap_aabb(&AABB2D::create(-3.0, -4.0, 3.0, 0.5), &query_filter, |shape_id| {
				aabb_list.push(shape_id.get_index().to_string());
				true
			});
			line_list.push(format!("step {} aabb {}", step_index, aabb_list.join(" ")));
			let mut point_list: Vec<String> = Vec::new();
			world.overlap_point(PhysicsVector2::create(0.2, -0.5), &query_filter, |shape_id| {
				point_list.push(shape_id.get_index().to_string());
				true
			});
			line_list.push(format!("step {} point {}", step_index, point_list.join(" ")));
			let mut shape_list: Vec<String> = Vec::new();
			world.overlap_shape(Circle2D::create(PhysicsVector2::zero(), 1.2), PhysicsVector2::create(-1.0, -1.5), 0.3, &query_filter, |shape_id| {
				shape_list.push(shape_id.get_index().to_string());
				shape_list.len() < 4
			});
			line_list.push(format!("step {} shape {}", step_index, shape_list.join(" ")));
		}
	}
	for body_id in world.get_body_ids() {
		let body = &world[body_id];
		let position = body.get_position();
		let rotation = body.get_rotation();
		let velocity = body.get_linear_velocity();
		let angular_velocity = body.get_angular_velocity();
		line_list.push(format!("body {} {} {} {} {} {} {} {} {}", body_id.get_index(), to_bits(position.x), to_bits(position.y), to_bits(rotation.cosine), to_bits(rotation.sine), to_bits(velocity.x), to_bits(velocity.y), to_bits(angular_velocity), if body.is_awake() {
			1
		}
		else {
			0
		}));
	}
	let tree = world.get_broad_phase_tree();
	line_list.push(format!("tree {} {} {}", tree.get_proxy_count(), tree.get_height(), to_bits(tree.get_area_ratio())));
	line_list.join("\n") + "\n"
}

//==============================================================================
/// 원본 vanilla.js 와 같은 장면, 같은 걸음에 비트까지 같은 결과.
//==============================================================================
#[test]
fn comparison_matches_vanilla_js() {
	let expected_text = include_str!("comparison.txt");
	let actual_text = run_comparison_scene();
	if expected_text != actual_text {
		let expected_line_list: Vec<&str> = expected_text.lines().collect();
		let actual_line_list: Vec<&str> = actual_text.lines().collect();
		for index in 0..expected_line_list.len().max(actual_line_list.len()) {
			let expected_line = expected_line_list.get(index).copied().unwrap_or("");
			let actual_line = actual_line_list.get(index).copied().unwrap_or("");
			assert_eq!(expected_line, actual_line, "{} 줄이 원본과 다르다", index + 1);
		}
	}
}

//==============================================================================
/// 결정론 삼각 함수는 표준 라이브러리와 1e-14 안이다.
//==============================================================================
#[test]
fn deterministic_math_accuracy() {
	for index in 0..20000 {
		let angle = (index as f64 / 20000.0 - 0.5) * 40.0;
		let rotation = PhysicsMath2D::compute_rotation(angle);
		assert!((rotation.cosine - angle.cos()).abs() < 1e-14, "cos {}", angle);
		assert!((rotation.sine - angle.sin()).abs() < 1e-14, "sin {}", angle);
		let y = (angle * 1.3).sin() * (1 + index % 7) as f64;
		let x = (angle * 0.7).cos() * (1 + index % 5) as f64;
		assert!((PhysicsMath2D::compute_atan2(y, x) - y.atan2(x)).abs() < 1e-14, "atan2 {} {}", y, x);
	}
	assert_eq!(PhysicsMath2D::compute_atan2(0.0, 0.0), 0.0);
	assert_eq!(PhysicsMath2D::compute_atan2(0.0, -1.0), PI);
	assert!((PhysicsMath2D::unwind_angle(-7.0) - (-7.0 + 2.0 * PI)).abs() < 1e-15);
}

//==============================================================================
/// 두 모양 한 쌍의 매니폴드를 몸 둘로 만들어 한 번 걸음해 본다. (중력 없음, 0 걸음)
//==============================================================================
fn collide_pair(geometry_a: ShapeGeometry2D, position_a: PhysicsVector2, angle_a: f64, geometry_b: ShapeGeometry2D, position_b: PhysicsVector2, angle_b: f64) -> PhysicsWorld2D {
	let mut world_definition = WorldDefinition2D::new(1.0);
	world_definition.gravity = PhysicsVector2::zero();
	let mut world = PhysicsWorld2D::new(&world_definition);
	let body_a = create_dynamic_body(&mut world, position_a.x, position_a.y, angle_a);
	world.create_shape(body_a, &ShapeDefinition2D::new(), geometry_a);
	let body_b = create_dynamic_body(&mut world, position_b.x, position_b.y, angle_b);
	world.create_shape(body_b, &ShapeDefinition2D::new(), geometry_b);
	world.step(0.0, 4);
	world
}

//==============================================================================
/// 원, 다각형, 캡슐, 선분 짝의 닿음점과 겹침 깊이.
//==============================================================================
#[test]
fn manifold_cases() {
	// 원과 원: 0.5 겹침.
	let world = collide_pair(Circle2D::create(PhysicsVector2::zero(), 1.0).into(), PhysicsVector2::zero(), 0.0, Circle2D::create(PhysicsVector2::zero(), 1.0).into(), PhysicsVector2::create(1.5, 0.0), 0.0);
	let contact = world.contact_list.iter().find(|contact| contact.is_alive).expect("닿음");
	assert_eq!(contact.manifold.point_count, 1);
	assert!((contact.manifold.points[0].separation + 0.5).abs() < 1e-12);
	assert!((contact.manifold.normal_x - 1.0).abs() < 1e-12);
	assert!((contact.manifold.points[0].point_x - 0.75).abs() < 1e-12);

	// 상자 위의 상자: 닿음점 둘, 겹침 0.1.
	let world = collide_pair(Polygon2D::make_box(1.0, 1.0).into(), PhysicsVector2::zero(), 0.0, Polygon2D::make_box(0.5, 0.5).into(), PhysicsVector2::create(0.2, -1.4), 0.0);
	let contact = world.contact_list.iter().find(|contact| contact.is_alive).expect("닿음");
	assert_eq!(contact.manifold.point_count, 2);
	for point_index in 0..2 {
		assert!((contact.manifold.points[point_index].separation + 0.1).abs() < 1e-12);
	}
	assert!((contact.manifold.normal_y + 1.0).abs() < 1e-12);

	// 상자 모서리에 원: 꼭짓점 쪽 법선은 대각선.
	let world = collide_pair(Polygon2D::make_box(1.0, 1.0).into(), PhysicsVector2::zero(), 0.0, Circle2D::create(PhysicsVector2::zero(), 0.5).into(), PhysicsVector2::create(1.3, 1.3), 0.0);
	let contact = world.contact_list.iter().find(|contact| contact.is_alive).expect("닿음");
	assert_eq!(contact.manifold.point_count, 1);
	let expected_separation = (0.3f64 * 0.3 * 2.0).sqrt() - 0.5;
	assert!((contact.manifold.points[0].separation - expected_separation).abs() < 1e-12);
	assert!((contact.manifold.normal_x - contact.manifold.normal_y).abs() < 1e-12);

	// 누운 캡슐 둘이 나란히 겹침: 닿음점 둘.
	let world = collide_pair(Capsule2D::create(PhysicsVector2::create(-1.0, 0.0), PhysicsVector2::create(1.0, 0.0), 0.25).into(), PhysicsVector2::zero(), 0.0, Capsule2D::create(PhysicsVector2::create(-1.0, 0.0), PhysicsVector2::create(1.0, 0.0), 0.25).into(), PhysicsVector2::create(0.5, 0.45), 0.0);
	let contact = world.contact_list.iter().find(|contact| contact.is_alive).expect("닿음");
	assert_eq!(contact.manifold.point_count, 2);
	assert!((contact.manifold.points[0].separation + 0.05).abs() < 1e-12);

	// 멀리 떨어진 상자는 닿음이 없다.
	let world = collide_pair(Polygon2D::make_box(1.0, 1.0).into(), PhysicsVector2::zero(), 0.0, Polygon2D::make_box(1.0, 1.0).into(), PhysicsVector2::create(5.0, 0.0), 0.7);
	assert_eq!(world.get_contact_count(), 0);
}

//==============================================================================
/// 상자 쌓기가 무너지지 않고 잠든다. (풀이 수렴)
//==============================================================================
#[test]
fn stack_converges_and_sleeps() {
	let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
	let ground_id = world.create_body(&BodyDefinition2D::new());
	world.create_shape(ground_id, &ShapeDefinition2D::new(), Segment2D::create(PhysicsVector2::create(-40.0, 0.0), PhysicsVector2::create(40.0, 0.0)));
	let row_count = 12;
	let mut top_id = ground_id;
	for row in 0..row_count {
		for column in 0..row_count - row {
			let x = (column as f64 - (row_count - row) as f64 / 2.0) + 0.5;
			let y = -0.5 - row as f64 * 1.05;
			top_id = create_dynamic_body(&mut world, x, y, 0.0);
			world.create_shape(top_id, &ShapeDefinition2D::new(), Polygon2D::make_box(0.45, 0.45));
		}
	}
	let mut sleep_step = None;
	for step_index in 0..900 {
		world.step(1.0 / 60.0, 4);
		if sleep_step.is_none() && world.get_awake_body_count() == 0 {
			sleep_step = Some(step_index);
		}
	}
	let top = world[top_id].get_position();
	let expected_top_y = -0.45 - (row_count - 1) as f64 * 0.9;
	assert!((top.x).abs() < 0.2, "꼭대기 x {}", top.x);
	assert!((top.y - expected_top_y).abs() < 0.08, "꼭대기 y {} 기대 {}", top.y, expected_top_y);
	assert!(sleep_step.is_some(), "잠들지 않음");
	assert!(world.get_broad_phase_tree().validate());
}

//==============================================================================
/// 회전 관절 사슬이 벌어지지 않고, 거리 용수철은 m g / k 만큼 늘어나고, 모터는 속도를 맞춘다.
//==============================================================================
#[test]
fn joints_hold() {
	let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
	let anchor_id = world.create_body(&BodyDefinition2D::new());
	let mut previous_id = anchor_id;
	let mut link_list = Vec::new();
	for index in 0..8 {
		let link_id = create_dynamic_body(&mut world, index as f64 + 0.5, 0.0, 0.0);
		world.create_shape(link_id, &ShapeDefinition2D::new(), Capsule2D::create(PhysicsVector2::create(-0.5, 0.0), PhysicsVector2::create(0.5, 0.0), 0.1));
		let mut joint_definition = RevoluteJointDefinition2D::new();
		joint_definition.body_id_a = Some(previous_id);
		joint_definition.body_id_b = Some(link_id);
		joint_definition.local_anchor_a = if previous_id == anchor_id {
			PhysicsVector2::zero()
		}
		else {
			PhysicsVector2::create(0.5, 0.0)
		};
		joint_definition.local_anchor_b = PhysicsVector2::create(-0.5, 0.0);
		world.create_revolute_joint(&joint_definition).expect("관절");
		previous_id = link_id;
		link_list.push(link_id);
	}
	for _ in 0..600 {
		world.step(1.0 / 60.0, 4);
	}
	// 오래 흔들린 뒤에는 거의 아래로 늘어지고 이음매가 붙어 있다.
	let mut maximum_gap: f64 = 0.0;
	for index in 1..link_list.len() {
		let point_a = world[link_list[index - 1]].get_world_point(PhysicsVector2::create(0.5, 0.0));
		let point_b = world[link_list[index]].get_world_point(PhysicsVector2::create(-0.5, 0.0));
		maximum_gap = maximum_gap.max(((point_a.x - point_b.x).powi(2) + (point_a.y - point_b.y).powi(2)).sqrt());
	}
	assert!(maximum_gap < 0.01, "이음매 벌어짐 {}", maximum_gap);

	// 거리 용수철. (원 반지름 0.2, 밀도 1 → 질량 0.04 파이, k = m (2 파이 2)^2)
	let bob_id = create_dynamic_body(&mut world, 30.0, 3.0, 0.0);
	world.create_shape(bob_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 0.2));
	let mut distance_definition = DistanceJointDefinition2D::new();
	distance_definition.body_id_a = Some(anchor_id);
	distance_definition.body_id_b = Some(bob_id);
	distance_definition.local_anchor_a = PhysicsVector2::create(30.0, 0.0);
	distance_definition.length = 2.0;
	distance_definition.enable_spring = true;
	distance_definition.hertz = 2.0;
	distance_definition.damping_ratio = 0.3;
	world.create_distance_joint(&distance_definition);
	for _ in 0..900 {
		world.step(1.0 / 60.0, 4);
	}
	let omega = 2.0 * PI * 2.0;
	let stretch = 10.0 / (omega * omega);
	assert!((world[bob_id].get_position().y - (2.0 + stretch)).abs() < 1e-3, "용수철 {}", world[bob_id].get_position().y);
}

//==============================================================================
/// 광선, 점, 상자, 모양 질의와 거르기.
//==============================================================================
#[test]
fn queries_and_filters() {
	let mut world_definition = WorldDefinition2D::new(1.0);
	world_definition.gravity = PhysicsVector2::zero();
	let mut world = PhysicsWorld2D::new(&world_definition);
	let body_id = world.create_body(&BodyDefinition2D::new());
	let box_id = world.create_shape(body_id, &ShapeDefinition2D::new(), Polygon2D::make_offset_box(1.0, 1.0, PhysicsVector2::create(5.0, 0.0), 0.0)).expect("상자");
	let mut circle_definition = ShapeDefinition2D::new();
	circle_definition.filter.category_bits = 0x0002;
	let circle_id = world.create_shape(body_id, &circle_definition, Circle2D::create(PhysicsVector2::create(10.0, 0.0), 1.0)).expect("원");
	let capsule_id = world.create_shape(body_id, &ShapeDefinition2D::new(), Capsule2D::create(PhysicsVector2::create(15.0, -1.0), PhysicsVector2::create(15.0, 1.0), 0.5)).expect("캡슐");

	let all_filter = QueryFilter2D::new();
	let hit = world.cast_ray_closest(PhysicsVector2::zero(), PhysicsVector2::create(20.0, 0.0), &all_filter).expect("맞음");
	assert_eq!(hit.shape_id, box_id);
	assert!((hit.point.x - 4.0).abs() < 1e-12);
	assert_eq!(hit.normal, PhysicsVector2::create(-1.0, 0.0));
	let mut hit_list = Vec::new();
	world.cast_ray(PhysicsVector2::zero(), PhysicsVector2::create(20.0, 0.0), &all_filter, |shape_id, point, _, _| {
		hit_list.push((shape_id, point.x));
		1.0
	});
	hit_list.sort_by(|first, second| first.1.partial_cmp(&second.1).expect("수"));
	assert_eq!(hit_list.len(), 3);
	assert_eq!(hit_list[1].0, circle_id);
	assert!((hit_list[2].1 - 14.5).abs() < 1e-12);
	assert_eq!(hit_list[2].0, capsule_id);

	// 2 번 분류를 빼면 원은 맞지 않는다.
	let mut no_circle_filter = QueryFilter2D::new();
	no_circle_filter.mask_bits = !0x0002;
	let mut count = 0;
	world.cast_ray(PhysicsVector2::create(8.0, 0.0), PhysicsVector2::create(4.0, 0.0), &no_circle_filter, |_, _, _, _| {
		count += 1;
		1.0
	});
	assert_eq!(count, 0);

	let mut point_hits = Vec::new();
	world.overlap_point(PhysicsVector2::create(15.3, 1.2), &all_filter, |shape_id| {
		point_hits.push(shape_id);
		true
	});
	assert_eq!(point_hits, vec![capsule_id]);
	let mut aabb_hits = Vec::new();
	world.overlap_aabb(&AABB2D::create(5.5, -0.1, 9.5, 0.1), &all_filter, |shape_id| {
		aabb_hits.push(shape_id);
		true
	});
	aabb_hits.sort_by_key(|shape_id| shape_id.get_index());
	assert_eq!(aabb_hits, vec![box_id, circle_id]);
	let mut shape_hits = Vec::new();
	world.overlap_shape(Circle2D::create(PhysicsVector2::zero(), 0.6), PhysicsVector2::create(10.0, 1.5), 0.0, &all_filter, |shape_id| {
		shape_hits.push(shape_id);
		true
	});
	assert_eq!(shape_hits, vec![circle_id]);

	// 무리 번호가 같은 음수면 마스크가 맞아도 지나친다.
	let mut first_filter = CollisionFilter2D::new();
	first_filter.group_index = -1;
	assert!(!first_filter.should_collide(&first_filter));
	first_filter.group_index = 2;
	let mut second_filter = CollisionFilter2D::new();
	second_filter.group_index = 2;
	second_filter.mask_bits = 0;
	assert!(first_filter.should_collide(&second_filter));
}

//==============================================================================
/// 센서 들어옴과 나감, 닿음 시작과 끝, 순간 이동하면 깨어남.
//==============================================================================
#[test]
fn sensor_contact_events_and_wake() {
	let mut world = PhysicsWorld2D::new(&WorldDefinition2D::new(1.0));
	let sensor_body_id = world.create_body(&BodyDefinition2D::new());
	let mut sensor_definition = ShapeDefinition2D::new();
	sensor_definition.is_sensor = true;
	let sensor_shape_id = world.create_shape(sensor_body_id, &sensor_definition, Polygon2D::make_offset_box(1.0, 1.0, PhysicsVector2::create(0.0, 3.0), 0.0)).expect("센서");
	let ground_id = world.create_body(&BodyDefinition2D::new());
	world.create_shape(ground_id, &ShapeDefinition2D::new(), Segment2D::create(PhysicsVector2::create(-5.0, 6.0), PhysicsVector2::create(5.0, 6.0)));
	let ball_id = create_dynamic_body(&mut world, 0.0, 0.0, 0.0);
	let ball_shape_id = world.create_shape(ball_id, &ShapeDefinition2D::new(), Circle2D::create(PhysicsVector2::zero(), 0.25)).expect("공");
	let mut sensor_begin_step = None;
	let mut sensor_end_step = None;
	let mut contact_begin_count = 0;
	for step_index in 0..240 {
		world.step(1.0 / 60.0, 4);
		for sensor_event in world.get_sensor_begin_events() {
			assert_eq!(sensor_event.sensor_shape_id, sensor_shape_id);
			assert_eq!(sensor_event.visitor_shape_id, ball_shape_id);
			sensor_begin_step = Some(step_index);
		}
		if !world.get_sensor_end_events().is_empty() {
			sensor_end_step = Some(step_index);
		}
		contact_begin_count += world.get_contact_begin_events().len();
	}
	assert!(sensor_begin_step.expect("센서 들어옴") < sensor_end_step.expect("센서 나감"));
	assert_eq!(contact_begin_count, 1);
	assert!(!world[ball_id].is_awake());
	world[ball_id].set_transform(PhysicsVector2::create(0.0, 2.0), 0.0);
	assert!(world[ball_id].is_awake());
	world.step(1.0 / 60.0, 4);
	assert_eq!(world.get_contact_end_events().len(), 1);
	world.destroy_body(ball_id);
	assert!(world.get_body(ball_id).is_none());
	assert!(world.get_shape(ball_shape_id).is_none());
	assert_eq!(world.get_body_count(), 2);
}
