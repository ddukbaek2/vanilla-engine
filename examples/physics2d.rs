//==============================================================================
// 2D 물리 초안 예제. (원본 vanilla.js 의 examples/physics2d 와 같은 장면)
// - 상자 피라미드, 회전 관절 다리, 거리 관절 진자, 바퀴 관절 수레, 용접 다이빙대, 튀는 공, 캡슐, 둥근 육각형,
//   운동학 발판, 센서 칸(안에 무언가 있으면 칠한다), 광선(맞은 점과 법선), RigidBody2D 컴포넌트로 움직이는 Paint 노드.
// - 프레임마다 한 걸음(1/60 초, 하위 걸음 4)을 돈다. PHYSICS2D_PRESTEP=180 을 주면 처음에 180 걸음을 돌고 멈춘다. (원본 ?prestep=180, 두 엔진의 찍은 화면을 견줄 때)
// - 실행: cargo run --example physics2d
//   검증용으로 창 없이 찍으려면 PHYSICS2D_PRESTEP=180 VANILLA_CAPTURE_PATH=<폴더>/shot_{frame}.png VANILLA_CAPTURE_FRAMES=10 VANILLA_CAPTURE_EXIT=1 을 준다.
//==============================================================================

use std::cell::RefCell;
use std::rc::Rc;
use vanilla::experimental::physics2d::{
	BodyDefinition2D, BodyId2D, BodyType2D, Capsule2D, ChainDefinition2D, Circle2D, DistanceJointDefinition2D, PhysicsDebugDraw2D, PhysicsMath2D, PhysicsVector2, PhysicsWorld2D, Polygon2D, QueryFilter2D, RevoluteJointDefinition2D, RigidBody2D, Segment2D, ShapeDefinition2D,
	WeldJointDefinition2D, WheelJointDefinition2D, WorldDefinition2D,
};
use vanilla::{Color, Engine, EngineConfiguration, Graphic, Paint, Pivot, Rect, Scene, SceneBase, Vector2};

// 화면 크기. (기준 해상도)
const VIEW_WIDTH: f64 = 1280.0;
const VIEW_HEIGHT: f64 = 720.0;
// 1 미터의 화소 수.
const PIXELS_PER_METER: f64 = 50.0;
// 한 걸음의 시간과 하위 걸음 수. (프레임마다 한 걸음, 두 엔진의 같은 프레임이 같은 걸음이 되게)
const STEP_SECONDS: f64 = 1.0 / 60.0;
const SUB_STEP_COUNT: u32 = 4;
// 운동학 발판이 방향을 바꾸는 걸음 수.
const PLATFORM_TURN_STEPS: u32 = 120;
// 광선.
const RAY_ORIGIN: PhysicsVector2 = PhysicsVector2::create(40.0, 250.0);
const RAY_TRANSLATION: PhysicsVector2 = PhysicsVector2::create(600.0, 360.0);
// 센서 칸.
const SENSOR_CENTER: PhysicsVector2 = PhysicsVector2::create(1130.0, 640.0);
const SENSOR_HALF_SIZE: PhysicsVector2 = PhysicsVector2::create(40.0, 40.0);
// 색.
const BACKGROUND_COLOR: Color = Color::create(0.106, 0.122, 0.165, 1.0);
const RAY_COLOR: Color = Color::create(1.0, 0.42, 0.38, 1.0);
const SENSOR_ACTIVE_COLOR: Color = Color::create(0.30, 0.80, 0.90, 0.35);
const NODE_BOX_COLOR: Color = Color::create(1.0, 0.44, 0.38, 1.0);

//==============================================================================
// 2D 물리 초안 견주기 씬.
//==============================================================================
struct Physics2DScene {
	scene_base: SceneBase,
	world: Rc<RefCell<PhysicsWorld2D>>,
	platform_id: Option<BodyId2D>,
	step_count: u32,
	sensor_visitor_count: i64,
	prestep_count: u32,
}

impl Physics2DScene {
	//==============================================================================
	// 동적 몸 하나 만들기.
	//==============================================================================
	fn create_dynamic_body(&self, x: f64, y: f64, angle: f64) -> BodyId2D {
		let mut body_definition = BodyDefinition2D::new();
		body_definition.body_type = BodyType2D::Dynamic;
		body_definition.position = PhysicsVector2::create(x, y);
		body_definition.angle = angle;
		self.world.borrow_mut().create_body(&body_definition)
	}

	//==============================================================================
	// 한 걸음. (발판 방향 바꾸기, 센서 안의 수 세기)
	//==============================================================================
	fn step_world(&mut self) {
		let mut world = self.world.borrow_mut();
		if self.step_count > 0 && self.step_count % PLATFORM_TURN_STEPS == 0 {
			if let Some(platform) = self.platform_id.and_then(|platform_id| world.get_body_mut(platform_id)) {
				let velocity = platform.get_linear_velocity();
				platform.set_linear_velocity(PhysicsVector2::create(-velocity.x, 0.0));
			}
		}
		world.step(STEP_SECONDS, SUB_STEP_COUNT);
		self.sensor_visitor_count += world.get_sensor_begin_events().len() as i64 - world.get_sensor_end_events().len() as i64;
		self.step_count += 1;
	}
}

impl Scene for Physics2DScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 초기화. (월드와 노드)
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let shape_definition = ShapeDefinition2D::new();

		// 땅, 왼쪽 비탈, 오른쪽 벽.
		let ground_id = self.world.borrow_mut().create_body(&BodyDefinition2D::new());
		{
			let mut world = self.world.borrow_mut();
			world.create_shape(ground_id, &shape_definition, Segment2D::create(PhysicsVector2::create(40.0, 680.0), PhysicsVector2::create(1240.0, 680.0)));
			world.create_shape(ground_id, &shape_definition, Segment2D::create(PhysicsVector2::create(1240.0, 680.0), PhysicsVector2::create(1240.0, 300.0)));
			let mut chain_definition = ChainDefinition2D::new();
			chain_definition.points = vec![PhysicsVector2::create(40.0, 420.0), PhysicsVector2::create(120.0, 560.0), PhysicsVector2::create(240.0, 680.0)];
			world.create_chain(ground_id, &chain_definition);
		}

		// 상자 피라미드.
		let row_count = 6;
		for row in 0..row_count {
			for column in 0..row_count - row {
				let x = 460.0 + (column as f64 - (row_count - row - 1) as f64 * 0.5) * 34.0;
				let y = 663.0 - row as f64 * 34.0;
				let body_id = self.create_dynamic_body(x, y, 0.0);
				self.world.borrow_mut().create_shape(body_id, &shape_definition, Polygon2D::make_box(16.0, 16.0));
			}
		}

		// 회전 관절 다리.
		let plank_count = 10;
		let mut previous_id = ground_id;
		for index in 0..plank_count {
			let plank_id = self.create_dynamic_body(613.0 + index as f64 * 26.0, 460.0, 0.0);
			let mut world = self.world.borrow_mut();
			world.create_shape(plank_id, &shape_definition, Polygon2D::make_box(13.0, 3.0));
			let mut joint_definition = RevoluteJointDefinition2D::new();
			joint_definition.body_id_a = Some(previous_id);
			joint_definition.body_id_b = Some(plank_id);
			joint_definition.local_anchor_a = if previous_id == ground_id {
				PhysicsVector2::create(600.0, 460.0)
			}
			else {
				PhysicsVector2::create(13.0, 0.0)
			};
			joint_definition.local_anchor_b = PhysicsVector2::create(-13.0, 0.0);
			world.create_revolute_joint(&joint_definition);
			previous_id = plank_id;
		}
		let mut last_joint_definition = RevoluteJointDefinition2D::new();
		last_joint_definition.body_id_a = Some(previous_id);
		last_joint_definition.body_id_b = Some(ground_id);
		last_joint_definition.local_anchor_a = PhysicsVector2::create(13.0, 0.0);
		last_joint_definition.local_anchor_b = PhysicsVector2::create(860.0, 460.0);
		self.world.borrow_mut().create_revolute_joint(&last_joint_definition);

		// 다리에 떨어지는 공.
		let mut ball_definition = ShapeDefinition2D::new();
		ball_definition.restitution = 0.5;
		for index in 0..6 {
			let body_id = self.create_dynamic_body(650.0 + index as f64 * 30.0, 200.0 + (index % 3) as f64 * 40.0, 0.0);
			self.world.borrow_mut().create_shape(body_id, &ball_definition, Circle2D::create(PhysicsVector2::zero(), 10.0));
		}

		// 캡슐과 둥근 육각형.
		for index in 0..2 {
			let body_id = self.create_dynamic_body(450.0 + index as f64 * 30.0, 300.0 - index as f64 * 40.0, 0.4 * index as f64);
			self.world.borrow_mut().create_shape(body_id, &shape_definition, Capsule2D::create(PhysicsVector2::create(-14.0, 0.0), PhysicsVector2::create(14.0, 0.0), 8.0));
		}
		let mut hexagon_point_list = Vec::new();
		for index in 0..6 {
			let rotation = PhysicsMath2D::compute_rotation(index as f64 * std::f64::consts::PI / 3.0);
			hexagon_point_list.push(PhysicsVector2::create(16.0 * rotation.cosine, 16.0 * rotation.sine));
		}
		let hexagon_id = self.create_dynamic_body(520.0, 200.0, 0.2);
		if let Some(hexagon) = Polygon2D::make_polygon(&hexagon_point_list, 3.0) {
			self.world.borrow_mut().create_shape(hexagon_id, &shape_definition, hexagon);
		}

		// 거리 관절 진자.
		let bob_id = self.create_dynamic_body(420.0, 120.0, 0.0);
		{
			let mut world = self.world.borrow_mut();
			world.create_shape(bob_id, &shape_definition, Circle2D::create(PhysicsVector2::zero(), 14.0));
			let mut pendulum_definition = DistanceJointDefinition2D::new();
			pendulum_definition.body_id_a = Some(ground_id);
			pendulum_definition.body_id_b = Some(bob_id);
			pendulum_definition.local_anchor_a = PhysicsVector2::create(300.0, 120.0);
			pendulum_definition.length = 120.0;
			world.create_distance_joint(&pendulum_definition);
		}

		// 바퀴 관절 수레.
		let chassis_id = self.create_dynamic_body(1000.0, 630.0, 0.0);
		self.world.borrow_mut().create_shape(chassis_id, &shape_definition, Polygon2D::make_box(40.0, 10.0));
		for index in 0..2 {
			let offset_x = if index == 0 {
				-28.0
			}
			else {
				28.0
			};
			let wheel_id = self.create_dynamic_body(1000.0 + offset_x, 652.0, 0.0);
			let mut world = self.world.borrow_mut();
			world.create_shape(wheel_id, &shape_definition, Circle2D::create(PhysicsVector2::zero(), 14.0));
			let mut wheel_definition = WheelJointDefinition2D::new();
			wheel_definition.body_id_a = Some(chassis_id);
			wheel_definition.body_id_b = Some(wheel_id);
			wheel_definition.local_anchor_a = PhysicsVector2::create(offset_x, 22.0);
			wheel_definition.hertz = 4.0;
			wheel_definition.damping_ratio = 0.7;
			wheel_definition.enable_motor = true;
			wheel_definition.motor_speed = 6.0;
			wheel_definition.max_motor_torque = 2000000.0;
			world.create_wheel_joint(&wheel_definition);
		}

		// 용접 다이빙대와 그 위에 떨어지는 공.
		let board_id = self.create_dynamic_body(1190.0, 300.0, 0.0);
		{
			let mut world = self.world.borrow_mut();
			world.create_shape(board_id, &shape_definition, Polygon2D::make_box(50.0, 5.0));
			let mut weld_definition = WeldJointDefinition2D::new();
			weld_definition.body_id_a = Some(ground_id);
			weld_definition.body_id_b = Some(board_id);
			weld_definition.local_anchor_a = PhysicsVector2::create(1240.0, 300.0);
			weld_definition.local_anchor_b = PhysicsVector2::create(50.0, 0.0);
			weld_definition.angular_hertz = 4.0;
			weld_definition.angular_damping_ratio = 0.3;
			world.create_weld_joint(&weld_definition);
		}
		let diver_id = self.create_dynamic_body(1160.0, 160.0, 0.0);
		self.world.borrow_mut().create_shape(diver_id, &ball_definition, Circle2D::create(PhysicsVector2::zero(), 12.0));

		// 센서 칸.
		{
			let mut world = self.world.borrow_mut();
			let sensor_body_id = world.create_body(&BodyDefinition2D::new());
			let mut sensor_definition = ShapeDefinition2D::new();
			sensor_definition.is_sensor = true;
			world.create_shape(sensor_body_id, &sensor_definition, Polygon2D::make_offset_box(SENSOR_HALF_SIZE.x, SENSOR_HALF_SIZE.y, SENSOR_CENTER, 0.0));

			// 운동학 발판.
			let mut platform_definition = BodyDefinition2D::new();
			platform_definition.body_type = BodyType2D::Kinematic;
			platform_definition.position = PhysicsVector2::create(1000.0, 360.0);
			platform_definition.linear_velocity = PhysicsVector2::create(80.0, 0.0);
			let platform_id = world.create_body(&platform_definition);
			world.create_shape(platform_id, &shape_definition, Polygon2D::make_box(50.0, 6.0));
			self.platform_id = Some(platform_id);
		}
		// 발판 위의 상자.
		let rider_id = self.create_dynamic_body(1000.0, 340.0, 0.0);
		self.world.borrow_mut().create_shape(rider_id, &shape_definition, Polygon2D::make_box(12.0, 12.0));

		// RigidBody2D 로 움직일 몸.
		let node_body_id = self.create_dynamic_body(180.0, 300.0, 0.3);
		self.world.borrow_mut().create_shape(node_body_id, &shape_definition, Polygon2D::make_box(20.0, 20.0));

		for _ in 0..self.prestep_count {
			self.step_world();
		}

		// RigidBody2D 로 움직이는 Paint 노드. (붙일 때 몸 자리에 놓인다)
		let root_id = self.scene_base.get_root_id();
		let node_tree = self.scene_base.get_node_tree_mut();
		let box_id = node_tree.create_child(root_id, "box");
		node_tree[box_id].set_pivot(Pivot::MIDDLE_CENTER);
		node_tree[box_id].set_content_size(Vector2::create(40.0, 40.0));
		node_tree[box_id].add_component(Paint::new()).set_color(NODE_BOX_COLOR);
		let mut rigid_body = RigidBody2D::new();
		rigid_body.set_body(self.world.clone(), node_body_id);
		node_tree[box_id].add_component(rigid_body);
	}

	//==============================================================================
	// 갱신. (멈추지 않았으면 프레임마다 한 걸음)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f64) {
		if self.prestep_count == 0 {
			self.step_world();
		}
		self.scene_base.tick(engine, time_delta);
	}

	//==============================================================================
	// 이전 출력. (배경)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		let view_manager = engine.get_view_manager();
		let canvas_native_size = view_manager.get_canvas_native_size();
		view_manager.apply_canvas_native_rect(graphic);
		graphic.set_fill_color(BACKGROUND_COLOR);
		graphic.draw_rect(Rect::create(0.0, 0.0, canvas_native_size.x, canvas_native_size.y));
		view_manager.apply_view_rect(graphic);
	}

	//==============================================================================
	// 출력. (센서 칸, 물리 모양, 광선, 노드)
	//==============================================================================
	fn draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		if self.sensor_visitor_count > 0 {
			graphic.set_fill_color(SENSOR_ACTIVE_COLOR);
			graphic.draw_rect(Rect::create(SENSOR_CENTER.x - SENSOR_HALF_SIZE.x, SENSOR_CENTER.y - SENSOR_HALF_SIZE.y, SENSOR_HALF_SIZE.x * 2.0, SENSOR_HALF_SIZE.y * 2.0));
		}
		let hit = {
			let mut world = self.world.borrow_mut();
			PhysicsDebugDraw2D::draw(&world, graphic, 2.0);
			world.cast_ray_closest(RAY_ORIGIN, RAY_TRANSLATION, &QueryFilter2D::new())
		};

		// 광선. (맞은 점까지 선, 맞은 점의 원, 법선)
		graphic.set_stroke_color(RAY_COLOR);
		graphic.set_fill_color(RAY_COLOR);
		let origin = RAY_ORIGIN.to_vector2();
		match hit {
			None => {
				let end = PhysicsVector2::create(RAY_ORIGIN.x + RAY_TRANSLATION.x, RAY_ORIGIN.y + RAY_TRANSLATION.y).to_vector2();
				graphic.draw_line(&[origin, end], 2.0);
			}
			Some(hit) => {
				let point = hit.point.to_vector2();
				graphic.draw_line(&[origin, point], 2.0);
				graphic.draw_circle(point, 5.0);
				let normal_end = PhysicsVector2::create(hit.point.x + hit.normal.x * 24.0, hit.point.y + hit.normal.y * 24.0).to_vector2();
				graphic.draw_line(&[point, normal_end], 2.0);
			}
		}
		self.scene_base.draw(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let prestep_count = std::env::var("PHYSICS2D_PRESTEP").ok().and_then(|text| text.parse::<u32>().ok()).unwrap_or(0);
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("Physics 2D");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.use_statistics = false;
	let engine = Engine::new(engine_configuration);
	let scene = Physics2DScene {
		scene_base: SceneBase::new(),
		world: Rc::new(RefCell::new(PhysicsWorld2D::new(&WorldDefinition2D::new(PIXELS_PER_METER)))),
		platform_id: None,
		step_count: 0,
		sensor_visitor_count: 0,
		prestep_count,
	};
	if let Err(error) = engine.run(Box::new(scene)) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
