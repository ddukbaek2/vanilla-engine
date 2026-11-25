//==============================================================================
//! 파티클 시스템 컴포넌트.
//! - 원본 effect/particlesystem.js 의 ParticleSystem. 이름, 기본값, 쓰임새(play / stop / emit / simulate / setStart...)는 원본을 따른다.
//! - 입자는 원본처럼 컴포넌트가 그리는 좌표(노드 content_size 영역의 왼쪽 위가 원점)의 원점에서 나온다. content_size 가 0 인 노드면 노드 자리다.
//!   끌개 자리(set_attractor)도 원본처럼 같은 좌표다.
//! - 낼 때 난수를 뽑는 차례와 셈의 차례도 원본과 같다. 같은 씨앗(set_random_seed)이면 원본과 같은 입자가 나온다.
//!   (시드 난수는 원본과 비트까지 같은 수열이다. 입자 셈은 엔진의 다른 곳처럼 f32 라서 원본(f64)과 끝자리만 다르다)
//! - 정점마다 색을 실어 draw_colored_quads 로 그린다. image 모양도 원본처럼 색 곡선과 시작 색(r, g, b)을 쓰지 않고 시작 알파 x (1 - 수명 비율) 로만 흐려진다.
//!   원본은 image 를 입자마다 따로 그리고, 러스트 판은 그림 텍스처로 한 번에 그린다. (그려지는 자리와 색은 같다)
//! - 원본과 다른 것:
//!   - 방출 모양, 렌더 모양, 합성은 원본의 문자열 대신 열거형(ParticleEmitterShape, ParticleRenderShape, BlendMode)이다. 서술(apply_description)은 원본 문자열을 읽는다.
//!   - 원본의 기본 인자(setStartLifetime(min, max = min) 같은)는 러스트에 없어 두 값을 다 받는다. emit 의 원점 보정은 Option 이다.
//!   - 월드 공간(set_world_space)에서 낼 때 더하는 노드 로컬 위치는 tick 에서 읽어 둔다. 러스트 컴포넌트는 tick 과 draw 밖에서 노드를 읽을 수 없어서다.
//!     원본처럼 낼 때의 노드 위치로 내려면 ParticleSystem::emit_in_tree(트리, 노드, 수, 보정)로 내거나, set_node_position 으로 위치를 넣고 emit 한다.
//!     (Timeline 의 emit 이벤트는 그 자리의 노드 위치를 넣고 낸다)
//!   - 정점 색 쿼드는 한 번에 그릴 수 있는 정점 수(Graphic 한도)씩 나눠 그린다. 원본은 배열을 늘려 한 번에 그린다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::seededrandom::SeededRandom;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic, PARTICLE_FLOATS_PER_VERTEX};
use crate::core::image::Image;
use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use std::rc::Rc;

//==============================================================================
/// 방출 모양. (원본 ParticleEmitterShape)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleEmitterShape {
	/// 노드 원점 한 점에서 모든 방향으로.
	Point,
	/// 반지름 안 무작위 위치에서 바깥 방향으로.
	Circle,
	/// 위쪽(-Y) 기준 반각 cone_angle 부채꼴 방향으로, 그 방향 반지름 안 위치에서.
	Cone,
	/// box_size 사각 영역 안 무작위 위치에서 위쪽(-Y)으로.
	Box,
	/// edge_width 가로 선분 위 무작위 위치에서 아래쪽(+Y)으로. (눈, 비처럼 위에서 뿌릴 때)
	Edge,
}

/// 서술의 방출 모양 이름. (원본 ParticleEmitterShape 값)
const EMITTER_SHAPE_NAME_TABLE: [(ParticleEmitterShape, &str); 5] = [
	(ParticleEmitterShape::Point, "point"),
	(ParticleEmitterShape::Circle, "circle"),
	(ParticleEmitterShape::Cone, "cone"),
	(ParticleEmitterShape::Box, "box"),
	(ParticleEmitterShape::Edge, "edge"),
];

// 방출 모양의 반사 값. (원본 ParticleEmitterShape 글)
crate::core::reflection::impl_reflect_names!(ParticleEmitterShape, EMITTER_SHAPE_NAME_TABLE);

//==============================================================================
/// 렌더 모양. (원본 ParticleRenderShape)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleRenderShape {
	/// 부드러운 원. (기본)
	Circle,
	/// 사각형. (색종이 조각처럼 도는 판)
	Rect,
	/// 진행 방향으로 늘인 띠. (비, 유성)
	Streak,
	/// set_image 로 넣은 그림.
	Image,
}

/// 서술의 렌더 모양 이름. (원본 ParticleRenderShape 값)
const RENDER_SHAPE_NAME_TABLE: [(ParticleRenderShape, &str); 4] = [
	(ParticleRenderShape::Circle, "circle"),
	(ParticleRenderShape::Rect, "rect"),
	(ParticleRenderShape::Streak, "streak"),
	(ParticleRenderShape::Image, "image"),
];

// 렌더 모양의 반사 값. (원본 ParticleRenderShape 글)
crate::core::reflection::impl_reflect_names!(ParticleRenderShape, RENDER_SHAPE_NAME_TABLE);

/// 쿼드 네 모서리의 텍스처 좌표. (왼쪽 위, 오른쪽 위, 오른쪽 아래, 왼쪽 아래)
const QUAD_TEXTURE_COORDINATES: [(f32, f32); 4] = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// 쿼드 한 장의 정점 수. (삼각형 둘)
const QUAD_VERTEX_COUNT: usize = 6;

//==============================================================================
/// 버스트 하나. (재생 시작부터 time 초에 count 개를 한 번에 낸다. 반복이면 duration 주기마다)
//==============================================================================
struct ParticleBurst {
	time: f32,
	count: usize,
	is_fired: bool,
}

//==============================================================================
/// 입자 하나. (원본 입자 객체의 칸)
//==============================================================================
struct Particle {
	x: f32,
	y: f32,
	velocity_x: f32,
	velocity_y: f32,
	age: f32,
	lifetime: f32,
	size: f32,
	rotation: f32,
	angular_velocity: f32,
	wobble_phase: f32,
	red: f32,
	green: f32,
	blue: f32,
	alpha: f32,
}

//==============================================================================
/// 파티클 시스템 컴포넌트.
/// - 노드에 붙이면 tick 마다 방출(초당 방출, 버스트)하고 입자를 적분해 정점 색 쿼드로 그린다. 좌표는 노드 content_size 영역의 왼쪽 위가 원점인 컴포넌트 좌표다.
///
/// # 예제
/// 한 번 터지는 불꽃 버스트
/// ```rust
/// use vanilla::{Color, NodeTree, ParticleEmitterShape, ParticleRenderShape, ParticleSystem};
///
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let spark_id = node_tree.create_child(root_id, "Spark");
/// let particle_system = node_tree[spark_id].add_component(ParticleSystem::new());
/// particle_system.set_looping(false);
/// particle_system.set_emission_rate(0.0);
/// particle_system.set_burst_list(vec![(0.0, 24)]);
/// particle_system.set_emitter_shape(ParticleEmitterShape::Circle);
/// particle_system.set_shape_radius(8.0);
/// particle_system.set_start_lifetime(0.6, 1.0);
/// particle_system.set_start_speed(120.0, 240.0);
/// particle_system.set_start_color(Color::create(1.0, 0.8, 0.2, 1.0), Color::create(1.0, 0.4, 0.1, 1.0));
/// particle_system.set_gravity(0.0, 300.0);
/// particle_system.set_render_shape(ParticleRenderShape::Streak);
/// particle_system.set_additive(true);
/// particle_system.set_random_seed(7);
/// particle_system.simulate(0.016);
/// assert_eq!(particle_system.get_particle_count(), 24);
/// particle_system.simulate(1.5);
/// assert_eq!(particle_system.get_particle_count(), 0);
/// assert!(particle_system.is_finished());
/// ```
//==============================================================================
pub struct ParticleSystem {
	component_base: ComponentBase,
	particle_list: Vec<Particle>,
	is_playing: bool,
	is_looping: bool,
	/// 루프 꺼짐일 때 방출이 이어지는 시간.
	duration: f32,
	play_elapsed_seconds: f32,
	max_particle_count: usize,

	/// 방출.
	emission_rate: f32,
	emission_accumulator: f32,
	burst_list: Vec<ParticleBurst>,
	emitter_shape: ParticleEmitterShape,
	shape_radius: f32,
	/// cone 전용 반각. (라디안)
	cone_angle_radian: f32,
	box_size: Vector2,
	edge_width: f32,

	/// 시작 범위.
	start_lifetime_min: f32,
	start_lifetime_max: f32,
	start_speed_min: f32,
	start_speed_max: f32,
	start_size_min: f32,
	start_size_max: f32,
	start_rotation_min: f32,
	start_rotation_max: f32,
	start_color_a: Color,
	start_color_b: Color,

	/// 수명 동안 변화.
	color_over_lifetime: Vec<(f32, Color)>,
	size_over_lifetime_start: f32,
	size_over_lifetime_end: f32,
	angular_velocity_min: f32,
	angular_velocity_max: f32,

	/// 물리.
	gravity: Vector2,
	damping: f32,

	/// 힘.
	attractor_position: Option<Vector2>,
	attractor_strength: f32,
	attractor_swirl: f32,
	wobble_amplitude: f32,
	wobble_frequency: f32,

	/// 렌더.
	render_shape: ParticleRenderShape,
	image: Option<Rc<Image>>,
	blend_mode: BlendMode,
	streak_scale: f32,
	is_world_space: bool,
	/// 월드 공간에서 낼 때 더하는 노드 로컬 위치. (tick 에서 읽어 둔다)
	node_position: Vector2,
	is_manual_tick: bool,
	random: Option<SeededRandom>,
	random_seed: Option<u32>,
}

#[crate::reflect]
impl ParticleSystem {
	//==============================================================================
	/// 생성. (원본 생성자의 기본값)
	///
	/// # 돌려줌
	/// 원본 기본값의 파티클 시스템. (재생 중, 반복, duration 1 초, 최대 512 개, 초당 20 개, Point 모양, 수명 1 초, 속력 60, 크기 6, 흰색, Circle 렌더, 보통 합성, 씨앗 없음)
	//==============================================================================
	pub fn new() -> ParticleSystem {
		ParticleSystem {
			component_base: ComponentBase::new(),
			particle_list: Vec::new(),
			is_playing: true,
			is_looping: true,
			duration: 1.0,
			play_elapsed_seconds: 0.0,
			max_particle_count: 512,
			emission_rate: 20.0,
			emission_accumulator: 0.0,
			burst_list: Vec::new(),
			emitter_shape: ParticleEmitterShape::Point,
			shape_radius: 0.0,
			cone_angle_radian: 0.5,
			box_size: Vector2::zero(),
			edge_width: 0.0,
			start_lifetime_min: 1.0,
			start_lifetime_max: 1.0,
			start_speed_min: 60.0,
			start_speed_max: 60.0,
			start_size_min: 6.0,
			start_size_max: 6.0,
			start_rotation_min: 0.0,
			start_rotation_max: 0.0,
			start_color_a: Color::white(),
			start_color_b: Color::white(),
			color_over_lifetime: Vec::new(),
			size_over_lifetime_start: 1.0,
			size_over_lifetime_end: 1.0,
			angular_velocity_min: 0.0,
			angular_velocity_max: 0.0,
			gravity: Vector2::zero(),
			damping: 0.0,
			attractor_position: None,
			attractor_strength: 0.0,
			attractor_swirl: 0.0,
			wobble_amplitude: 0.0,
			wobble_frequency: 4.0,
			render_shape: ParticleRenderShape::Circle,
			image: None,
			blend_mode: BlendMode::Normal,
			streak_scale: 0.06,
			is_world_space: false,
			node_position: Vector2::zero(),
			is_manual_tick: false,
			random: None,
			random_seed: None,
		}
	}

	//==============================================================================
	/// 재생.
	//==============================================================================
	pub fn play(&mut self) {
		self.is_playing = true;
		self.play_elapsed_seconds = 0.0;
		for burst in self.burst_list.iter_mut() {
			burst.is_fired = false;
		}
	}

	//==============================================================================
	/// 정지. (is_clearing 이면 살아 있는 입자도 지운다)
	///
	/// # 인자
	/// - `is_clearing`: 참이면 살아 있는 입자와 쌓인 방출량을 지우고 씨앗 난수를 처음으로 되돌린다. 거짓이면 내기만 멈추고 남은 입자는 수명대로 산다.
	//==============================================================================
	pub fn stop(&mut self, is_clearing: bool) {
		self.is_playing = false;
		if is_clearing {
			self.particle_list.clear();
			self.emission_accumulator = 0.0;
			if let (Some(random), Some(random_seed)) = (self.random.as_mut(), self.random_seed) {
				random.set_seed(random_seed);
			}
		}
	}

	//==============================================================================
	/// 한 번에 여러 개 방출. (버튼 클릭 등 이벤트성 연출, origin_offset 은 방출 원점 보정, 노드 로컬 기준)
	/// - 재생 중이 아니어도 낸다.
	///
	/// # 인자
	/// - `count`: 낼 입자 수. (최대 입자 수를 넘는 것은 내지 않는다)
	/// - `origin_offset`: 방출 원점 보정. (컴포넌트 좌표) None 이면 원점.
	///
	/// # 예제
	/// 최대 수까지 내고 원점을 옮겨 내기
	/// ```rust
	/// use vanilla::{ParticleSystem, Vector2};
	///
	/// let mut particle_system = ParticleSystem::new();
	/// particle_system.set_max_particle_count(12);
	/// particle_system.emit(10, None);
	/// particle_system.emit(5, Some(Vector2::create(0.0, -20.0)));
	/// assert_eq!(particle_system.get_particle_count(), 12);
	/// assert_eq!(particle_system.get_particle_position_list()[11], Vector2::create(0.0, -20.0));
	/// ```
	//==============================================================================
	pub fn emit(&mut self, count: usize, origin_offset: Option<Vector2>) {
		for _ in 0..count {
			self.spawn_particle(origin_offset);
		}
	}

	//==============================================================================
	/// 노드의 지금 위치로 여러 개 방출. (원본 emit 처럼 낼 때 노드 로컬 위치를 읽는다. 노드나 파티클 시스템이 없으면 false)
	///
	/// # 인자
	/// - `node_tree`: 노드가 든 노드 트리.
	/// - `node_id`: ParticleSystem 이 붙은 노드. (첫 번째 ParticleSystem 으로 낸다)
	/// - `count`: 낼 입자 수.
	/// - `origin_offset`: 방출 원점 보정. (컴포넌트 좌표) None 이면 원점.
	///
	/// # 돌려줌
	/// 냈으면 true. 노드가 없거나 ParticleSystem 이 붙어 있지 않으면 false.
	///
	/// # 예제
	/// 월드 공간 입자를 노드 자리에서 내기
	/// ```rust
	/// use vanilla::{NodeTree, ParticleSystem, Vector2};
	///
	/// let mut node_tree = NodeTree::new();
	/// let root_id = node_tree.get_root_id();
	/// let spark_id = node_tree.create_child(root_id, "Spark");
	/// node_tree[spark_id].set_local_position(Vector2::create(100.0, 50.0));
	/// let particle_system = node_tree[spark_id].add_component(ParticleSystem::new());
	/// particle_system.set_world_space(true);
	/// assert!(ParticleSystem::emit_in_tree(&mut node_tree, spark_id, 3, None));
	/// let position_list = node_tree[spark_id].get_component::<ParticleSystem>().map(|particle_system| particle_system.get_particle_position_list()).unwrap_or_default();
	/// assert_eq!(position_list, vec![Vector2::create(100.0, 50.0); 3]);
	/// assert!(!ParticleSystem::emit_in_tree(&mut node_tree, root_id, 3, None));
	/// ```
	//==============================================================================
	pub fn emit_in_tree(node_tree: &mut NodeTree, node_id: NodeId, count: usize, origin_offset: Option<Vector2>) -> bool {
		let node = match node_tree.get_node_mut(node_id) {
			Some(node) => {
				node
			}
			None => {
				return false;
			}
		};
		let node_position = node.get_local_position();
		match node.get_component_mut::<ParticleSystem>() {
			Some(particle_system) => {
				particle_system.set_node_position(node_position);
				particle_system.emit(count, origin_offset);
				true
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 월드 공간에서 낼 때 더할 노드 로컬 위치 설정. (tick 이 읽어 두는 값을 낼 자리에서 고칠 때)
	///
	/// # 인자
	/// - `node_position`: 노드 로컬 위치. (부모 좌표, 다음 tick 이 노드 위치로 다시 덮는다)
	//==============================================================================
	pub fn set_node_position(&mut self, node_position: Vector2) {
		self.node_position = node_position;
	}

	//==============================================================================
	/// 시간 진행. (방출 + 적분, 수동 tick 이면 밖에서 부른다)
	/// - 재생 중이 아니면 내지 않고 남은 입자만 움직인다. 한 번에 받은 시간을 한 걸음으로 적분한다.
	///
	/// # 인자
	/// - `time_delta`: 진행할 시간. (초)
	///
	/// # 예제
	/// 수동 tick 으로 0.5초 진행하기
	/// ```rust
	/// use vanilla::ParticleSystem;
	///
	/// let mut particle_system = ParticleSystem::new();
	/// particle_system.set_manual_tick(true);
	/// particle_system.set_emission_rate(10.0);
	/// particle_system.simulate(0.5);
	/// assert_eq!(particle_system.get_particle_count(), 5);
	/// ```
	//==============================================================================
	pub fn simulate(&mut self, time_delta: f32) {
		// 방출.
		if self.is_playing {
			let previous_elapsed = self.play_elapsed_seconds;
			self.play_elapsed_seconds += time_delta;

			let is_emit_window = self.is_looping || self.play_elapsed_seconds <= self.duration;
			if is_emit_window && self.emission_rate > 0.0 {
				self.emission_accumulator += self.emission_rate * time_delta;
				while self.emission_accumulator >= 1.0 {
					self.emission_accumulator -= 1.0;
					self.spawn_particle(None);
				}
			}

			// 버스트. (루프면 duration 주기로 반복)
			for burst_index in 0..self.burst_list.len() {
				let burst_time = self.burst_list[burst_index].time;
				let mut current_time = self.play_elapsed_seconds;
				let mut previous_time = previous_elapsed;
				if self.is_looping && self.duration > 0.0 {
					current_time = self.play_elapsed_seconds % self.duration;
					previous_time = previous_elapsed % self.duration;
					if current_time < previous_time {
						self.burst_list[burst_index].is_fired = false;
					}
				}
				if !self.burst_list[burst_index].is_fired && previous_time <= burst_time && current_time >= burst_time {
					self.burst_list[burst_index].is_fired = true;
					let burst_count = self.burst_list[burst_index].count;
					self.emit(burst_count, None);
				}
			}
		}

		// 적분. (수명이 다한 것은 빼고, 살아 있는 것은 차례 그대로)
		let gravity = self.gravity;
		let attractor_position = self.attractor_position;
		let attractor_strength = self.attractor_strength;
		let attractor_swirl = self.attractor_swirl;
		let damping = self.damping;
		self.particle_list.retain_mut(|particle| {
			particle.age += time_delta;
			if particle.age >= particle.lifetime {
				return false;
			}
			particle.velocity_x += gravity.x * time_delta;
			particle.velocity_y += gravity.y * time_delta;
			if let Some(attractor_position) = attractor_position {
				// 가까이 붙어 튀지 않게 거리는 4 아래로 내리지 않는다.
				let to_center_x = attractor_position.x - particle.x;
				let to_center_y = attractor_position.y - particle.y;
				let center_distance = (to_center_x * to_center_x + to_center_y * to_center_y).sqrt().max(4.0);
				let direction_x = to_center_x / center_distance;
				let direction_y = to_center_y / center_distance;
				particle.velocity_x += direction_x * attractor_strength * time_delta;
				particle.velocity_y += direction_y * attractor_strength * time_delta;
				particle.velocity_x += -direction_y * attractor_swirl * time_delta;
				particle.velocity_y += direction_x * attractor_swirl * time_delta;
			}
			if damping > 0.0 {
				let damping_blend = (1.0 - damping * time_delta).max(0.0);
				particle.velocity_x *= damping_blend;
				particle.velocity_y *= damping_blend;
			}
			particle.x += particle.velocity_x * time_delta;
			particle.y += particle.velocity_y * time_delta;
			particle.rotation += particle.angular_velocity * time_delta;
			true
		});
	}

	//==============================================================================
	/// 파티클 하나 생성. (최대 수면 내지 않는다, 원본과 같은 차례로 난수를 뽑는다)
	//==============================================================================
	fn spawn_particle(&mut self, origin_offset: Option<Vector2>) {
		if self.particle_list.len() >= self.max_particle_count {
			return;
		}
		let mut spawn_x = 0.0;
		let mut spawn_y = 0.0;
		let mut direction_x = 0.0;
		let mut direction_y = -1.0;
		match self.emitter_shape {
			ParticleEmitterShape::Circle => {
				let angle = self.random_range(0.0, math::PI * 2.0);
				let radius = self.shape_radius * self.random_range(0.0, 1.0).sqrt();
				spawn_x = angle.cos() * radius;
				spawn_y = angle.sin() * radius;
				direction_x = angle.cos();
				direction_y = angle.sin();
			}
			ParticleEmitterShape::Cone => {
				let angle = -math::PI * 0.5 + self.random_range(-self.cone_angle_radian, self.cone_angle_radian);
				let radius = self.random_range(0.0, self.shape_radius);
				spawn_x = angle.cos() * radius;
				spawn_y = angle.sin() * radius;
				direction_x = angle.cos();
				direction_y = angle.sin();
			}
			ParticleEmitterShape::Box => {
				spawn_x = self.random_range(-self.box_size.x * 0.5, self.box_size.x * 0.5);
				spawn_y = self.random_range(-self.box_size.y * 0.5, self.box_size.y * 0.5);
			}
			ParticleEmitterShape::Edge => {
				spawn_x = self.random_range(-self.edge_width * 0.5, self.edge_width * 0.5);
				direction_y = 1.0;
			}
			ParticleEmitterShape::Point => {
				let angle = self.random_range(0.0, math::PI * 2.0);
				direction_x = angle.cos();
				direction_y = angle.sin();
			}
		}

		if let Some(origin_offset) = origin_offset {
			spawn_x += origin_offset.x;
			spawn_y += origin_offset.y;
		}

		// 월드 공간이면 노드의 현재 위치를 더해 고정한다.
		if self.is_world_space {
			spawn_x += self.node_position.x;
			spawn_y += self.node_position.y;
		}

		let speed = self.random_range(self.start_speed_min, self.start_speed_max);
		let color_blend = self.random_range(0.0, 1.0);
		let lifetime = self.random_range(self.start_lifetime_min, self.start_lifetime_max);
		let size = self.random_range(self.start_size_min, self.start_size_max);
		let rotation = self.random_range(self.start_rotation_min, self.start_rotation_max);
		let angular_velocity = self.random_range(self.angular_velocity_min, self.angular_velocity_max);
		let wobble_phase = self.random_range(0.0, math::PI * 2.0);
		let color_a = self.start_color_a;
		let color_b = self.start_color_b;
		self.particle_list.push(Particle {
			x: spawn_x,
			y: spawn_y,
			velocity_x: direction_x * speed,
			velocity_y: direction_y * speed,
			age: 0.0,
			lifetime,
			size,
			rotation,
			angular_velocity,
			wobble_phase,
			red: color_a.red + (color_b.red - color_a.red) * color_blend,
			green: color_a.green + (color_b.green - color_a.green) * color_blend,
			blue: color_a.blue + (color_b.blue - color_a.blue) * color_blend,
			alpha: color_a.alpha + (color_b.alpha - color_a.alpha) * color_blend,
		});
	}

	//==============================================================================
	/// 수명 비율에 따른 색 계산. (시작 색에 색 곡선을 곱한다, 알파는 0 ~ 1)
	//==============================================================================
	fn evaluate_color(&self, particle: &Particle, life_ratio: f32) -> Color {
		let mut red = particle.red;
		let mut green = particle.green;
		let mut blue = particle.blue;
		let mut alpha = particle.alpha;
		let gradient = &self.color_over_lifetime;
		if !gradient.is_empty() {
			let mut previous_key = gradient[0];
			let mut next_key = gradient[gradient.len() - 1];
			for key in gradient.iter() {
				if key.0 <= life_ratio {
					previous_key = *key;
				}
				else {
					next_key = *key;
					break;
				}
			}
			let key_span = (next_key.0 - previous_key.0).max(0.0001);
			let key_blend = math::clamp01((life_ratio - previous_key.0) / key_span);
			red *= previous_key.1.red + (next_key.1.red - previous_key.1.red) * key_blend;
			green *= previous_key.1.green + (next_key.1.green - previous_key.1.green) * key_blend;
			blue *= previous_key.1.blue + (next_key.1.blue - previous_key.1.blue) * key_blend;
			alpha *= previous_key.1.alpha + (next_key.1.alpha - previous_key.1.alpha) * key_blend;
		}
		Color::create(red, green, blue, math::clamp01(alpha))
	}

	//==============================================================================
	/// 초당 방출 수 설정.
	///
	/// # 인자
	/// - `emission_rate`: 초당 내는 입자 수. (0 이하면 꾸준히 내지 않는다, 기본 20. 반복이 아니면 duration 동안만 낸다)
	//==============================================================================
	pub fn set_emission_rate(&mut self, emission_rate: f32) {
		self.emission_rate = emission_rate;
	}

	//==============================================================================
	/// 버스트 목록 설정. ((재생 시작부터 초, 개수) 목록)
	///
	/// # 인자
	/// - `burst_list`: (재생 시작부터 초, 개수) 목록. 이전 목록을 바꾸고 낸 표시를 지운다. 반복이면 duration 주기마다 다시 낸다.
	//==============================================================================
	pub fn set_burst_list(&mut self, burst_list: Vec<(f32, usize)>) {
		self.burst_list = burst_list.into_iter().map(|(time, count)| ParticleBurst { time, count, is_fired: false }).collect();
	}

	//==============================================================================
	/// 방출 모양 설정.
	///
	/// # 인자
	/// - `emitter_shape`: 낼 자리와 방향을 정하는 모양. (기본 Point)
	//==============================================================================
	pub fn set_emitter_shape(&mut self, emitter_shape: ParticleEmitterShape) {
		self.emitter_shape = emitter_shape;
	}

	//==============================================================================
	/// circle / cone 반지름.
	///
	/// # 인자
	/// - `shape_radius`: circle 과 cone 의 방출 반지름. (컴포넌트 좌표, 기본 0)
	//==============================================================================
	pub fn set_shape_radius(&mut self, shape_radius: f32) {
		self.shape_radius = shape_radius;
	}

	//==============================================================================
	/// cone 반각. (라디안)
	///
	/// # 인자
	/// - `cone_angle_radian`: 위쪽(-Y) 기준 부채꼴 반각. (라디안, 기본 0.5)
	//==============================================================================
	pub fn set_cone_angle(&mut self, cone_angle_radian: f32) {
		self.cone_angle_radian = cone_angle_radian;
	}

	//==============================================================================
	/// box 방출 영역 크기 설정. (원점이 가운데)
	///
	/// # 인자
	/// - `width`: 영역 너비. (컴포넌트 좌표, 기본 0)
	/// - `height`: 영역 높이. (컴포넌트 좌표, 기본 0)
	//==============================================================================
	pub fn set_box_size(&mut self, width: f32, height: f32) {
		self.box_size = Vector2::create(width, height);
	}

	//==============================================================================
	/// edge 방출 모양의 가로 길이.
	///
	/// # 인자
	/// - `edge_width`: 가로 선분 길이. (컴포넌트 좌표, 원점이 가운데, 기본 0)
	//==============================================================================
	pub fn set_edge_width(&mut self, edge_width: f32) {
		self.edge_width = edge_width;
	}

	//==============================================================================
	/// 시작 수명 범위 설정. (낼 때 둘 사이 무작위)
	///
	/// # 인자
	/// - `minimum_seconds`: 가장 짧은 수명. (초, 기본 1)
	/// - `maximum_seconds`: 가장 긴 수명. (초, 기본 1, 최소와 같으면 고정)
	//==============================================================================
	pub fn set_start_lifetime(&mut self, minimum_seconds: f32, maximum_seconds: f32) {
		self.start_lifetime_min = minimum_seconds;
		self.start_lifetime_max = maximum_seconds;
	}

	//==============================================================================
	/// 시작 속력 범위 설정. (낼 때 둘 사이 무작위)
	///
	/// # 인자
	/// - `minimum_speed`: 가장 느린 속력. (컴포넌트 좌표 초당, 기본 60)
	/// - `maximum_speed`: 가장 빠른 속력. (컴포넌트 좌표 초당, 기본 60)
	//==============================================================================
	pub fn set_start_speed(&mut self, minimum_speed: f32, maximum_speed: f32) {
		self.start_speed_min = minimum_speed;
		self.start_speed_max = maximum_speed;
	}

	//==============================================================================
	/// 시작 크기 범위 설정. (낼 때 둘 사이 무작위)
	///
	/// # 인자
	/// - `minimum_size`: 가장 작은 지름. (컴포넌트 좌표, 기본 6)
	/// - `maximum_size`: 가장 큰 지름. (컴포넌트 좌표, 기본 6)
	//==============================================================================
	pub fn set_start_size(&mut self, minimum_size: f32, maximum_size: f32) {
		self.start_size_min = minimum_size;
		self.start_size_max = maximum_size;
	}

	//==============================================================================
	/// 시작 회전 범위 설정. (낼 때 둘 사이 무작위, rect 와 image 렌더만 돈다)
	///
	/// # 인자
	/// - `minimum_radian`: 가장 작은 회전. (라디안, 기본 0)
	/// - `maximum_radian`: 가장 큰 회전. (라디안, 기본 0)
	//==============================================================================
	pub fn set_start_rotation(&mut self, minimum_radian: f32, maximum_radian: f32) {
		self.start_rotation_min = minimum_radian;
		self.start_rotation_max = maximum_radian;
	}

	//==============================================================================
	/// 시작 색. (둘 사이 무작위)
	///
	/// # 인자
	/// - `color_a`: 한 끝 색. (Color 나 "#rrggbb" 같은 글, 기본 흰색)
	/// - `color_b`: 다른 끝 색. (낼 때 두 색 사이를 무작위 비율로 섞는다, 기본 흰색)
	//==============================================================================
	pub fn set_start_color(&mut self, color_a: impl Into<Color>, color_b: impl Into<Color>) {
		self.start_color_a = color_a.into();
		self.start_color_b = color_b.into();
	}

	//==============================================================================
	/// 수명에 따른 색 곡선. (수명 비율 0 ~ 1 과 색의 쌍 목록, 시작 색에 곱한다)
	///
	/// # 인자
	/// - `gradient`: (수명 비율 0 ~ 1, 색) 목록. 비율 오름차순으로 준다. 사이는 선형으로 섞고 빈 목록이면 시작 색 그대로다. (image 렌더는 쓰지 않는다)
	//==============================================================================
	pub fn set_color_over_lifetime(&mut self, gradient: Vec<(f32, Color)>) {
		self.color_over_lifetime = gradient;
	}

	//==============================================================================
	/// 수명에 따른 크기 배율 설정. (태어날 때에서 수명 끝까지 선형)
	///
	/// # 인자
	/// - `start_scale`: 태어날 때 배율. (기본 1)
	/// - `end_scale`: 수명 끝 배율. (기본 1)
	//==============================================================================
	pub fn set_size_over_lifetime(&mut self, start_scale: f32, end_scale: f32) {
		self.size_over_lifetime_start = start_scale;
		self.size_over_lifetime_end = end_scale;
	}

	//==============================================================================
	/// 회전 속도 범위 설정. (낼 때 둘 사이 무작위)
	///
	/// # 인자
	/// - `minimum_radian_per_second`: 가장 느린 회전 속도. (초당 라디안, 기본 0)
	/// - `maximum_radian_per_second`: 가장 빠른 회전 속도. (초당 라디안, 기본 0)
	//==============================================================================
	pub fn set_angular_velocity(&mut self, minimum_radian_per_second: f32, maximum_radian_per_second: f32) {
		self.angular_velocity_min = minimum_radian_per_second;
		self.angular_velocity_max = maximum_radian_per_second;
	}

	//==============================================================================
	/// 중력 가속도 설정.
	///
	/// # 인자
	/// - `gravity_x`: 가로 가속도. (컴포넌트 좌표 초당 초당, 기본 0)
	/// - `gravity_y`: 세로 가속도. (아래가 +, 기본 0)
	//==============================================================================
	pub fn set_gravity(&mut self, gravity_x: f32, gravity_y: f32) {
		self.gravity = Vector2::create(gravity_x, gravity_y);
	}

	//==============================================================================
	/// 초당 감쇠 계수. (0 = 없음)
	///
	/// # 인자
	/// - `damping`: 감쇠 계수. (simulate 마다 속도에 max(1 - 계수 x time_delta, 0) 을 곱한다. 0 이하면 없음, 기본 0)
	//==============================================================================
	pub fn set_damping(&mut self, damping: f32) {
		self.damping = damping;
	}

	//==============================================================================
	/// 렌더 모양 설정. (기본 circle)
	///
	/// # 인자
	/// - `render_shape`: 입자를 그릴 모양.
	//==============================================================================
	pub fn set_render_shape(&mut self, render_shape: ParticleRenderShape) {
		self.render_shape = render_shape;
	}

	//==============================================================================
	/// 입자 이미지. (렌더 모양이 image 일 때 쓴다)
	///
	/// # 인자
	/// - `image`: Image 렌더로 그릴 그림. None 이면 Image 렌더도 흰 사각형에 입자 색으로 그린다.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	/// 더하기 합성 여부. (set_blend_mode(Lighter) 의 설탕)
	///
	/// # 인자
	/// - `is_additive`: 참이면 Lighter, 거짓이면 Normal 합성.
	//==============================================================================
	pub fn set_additive(&mut self, is_additive: bool) {
		self.blend_mode = if is_additive {
			BlendMode::Lighter
		}
		else {
			BlendMode::Normal
		};
	}

	//==============================================================================
	/// 합성 설정.
	///
	/// # 인자
	/// - `blend_mode`: 입자를 그릴 합성. (기본 Normal, 그린 뒤 이전 합성으로 되돌린다)
	//==============================================================================
	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		self.blend_mode = blend_mode;
	}

	//==============================================================================
	/// 끌개 설정. (컴포넌트 좌표의 자리, 중심 쪽 가속(음수면 밀어냄), 접선 쪽 가속(소용돌이))
	///
	/// # 인자
	/// - `x`: 끌개 x. (컴포넌트 좌표)
	/// - `y`: 끌개 y. (컴포넌트 좌표)
	/// - `strength`: 중심 쪽 가속. (초당 초당, 음수면 밀어낸다. 거리는 4 아래로 보지 않는다)
	/// - `swirl`: 접선 쪽 가속. (초당 초당, 소용돌이)
	//==============================================================================
	pub fn set_attractor(&mut self, x: f32, y: f32, strength: f32, swirl: f32) {
		self.attractor_position = Some(Vector2::create(x, y));
		self.attractor_strength = strength;
		self.attractor_swirl = swirl;
	}

	//==============================================================================
	/// 끌개 해제.
	//==============================================================================
	pub fn clear_attractor(&mut self) {
		self.attractor_position = None;
	}

	//==============================================================================
	/// 흔들림 설정. (가로로 살랑이는 폭, 초당 각속도. 원본 기본 각속도는 4)
	///
	/// # 인자
	/// - `amplitude`: 가로로 살랑이는 폭. (컴포넌트 좌표, 0 이하면 없음. 그릴 때만 더하고 자리에는 넣지 않는다)
	/// - `frequency`: 살랑이는 각속도. (초당 라디안, 기본 4)
	//==============================================================================
	pub fn set_wobble(&mut self, amplitude: f32, frequency: f32) {
		self.wobble_amplitude = amplitude;
		self.wobble_frequency = frequency;
	}

	//==============================================================================
	/// streak 길이 비율. (띠의 길이 = 속도 x 이 값)
	///
	/// # 인자
	/// - `streak_scale`: 속력에 곱할 띠 길이 비율. (초, 기본 0.06)
	//==============================================================================
	pub fn set_streak_scale(&mut self, streak_scale: f32) {
		self.streak_scale = streak_scale;
	}

	//==============================================================================
	/// 월드 공간 설정. (켜면 낸 뒤 노드가 움직여도 입자는 그 자리에 남는다)
	///
	/// # 인자
	/// - `is_world_space`: 참이면 낼 때 노드 로컬 위치를 더하고 그릴 때 노드 위치를 뺀다. (기본 거짓)
	//==============================================================================
	pub fn set_world_space(&mut self, is_world_space: bool) {
		self.is_world_space = is_world_space;
	}

	//==============================================================================
	/// 반복 설정.
	///
	/// # 인자
	/// - `is_looping`: 참이면 끝없이 내고 버스트를 duration 주기로 다시 낸다. 거짓이면 duration 동안만 낸다. (기본 참)
	//==============================================================================
	pub fn set_looping(&mut self, is_looping: bool) {
		self.is_looping = is_looping;
	}

	//==============================================================================
	/// 루프 꺼짐일 때 방출 시간. (버스트 반복 주기)
	///
	/// # 인자
	/// - `duration`: 방출 시간이자 버스트 반복 주기. (초, 기본 1)
	//==============================================================================
	pub fn set_duration(&mut self, duration: f32) {
		self.duration = duration;
	}

	//==============================================================================
	/// 최대 입자 수 설정.
	///
	/// # 인자
	/// - `max_particle_count`: 살아 있는 입자의 최대 수. (이만큼 있으면 새로 내지 않는다, 기본 512)
	//==============================================================================
	pub fn set_max_particle_count(&mut self, max_particle_count: usize) {
		self.max_particle_count = max_particle_count;
	}

	//==============================================================================
	/// 서술(JSON) 적용. (원본 applyDescription. vfx 애셋 로드, 파티클 편집기 공용. 준 항목만 반영한다. 색은 [r, g, b, a] 배열)
	/// - attractor 는 { x, y, strength, swirl } 이고 null 이면 끌개를 푼다. wobble 은 [폭, 각속도] 이고 각속도를 빼면 4 다.
	/// - [최소, 최대] 쌍에서 최대를 빼면 원본 기본 인자처럼 최소와 같다.
	///
	/// # 인자
	/// - `description`: 서술 객체. (원본 키 looping, duration, maxParticleCount, emissionRate, bursts, shape, shapeRadius, coneAngle, boxSize, edgeWidth, lifetime, speed, size, rotation, angularVelocity, startColorA, startColorB, colorOverLifetime, sizeOverLifetime, gravity, damping, attractor, wobble, renderShape, blendMode, streakScale, worldSpace. 모르는 이름의 모양과 합성은 무시한다)
	///
	/// # 예제
	/// 서술로 한 번 터지는 버스트 정하기
	/// ```rust
	/// use serde_json::json;
	/// use vanilla::ParticleSystem;
	///
	/// let mut particle_system = ParticleSystem::new();
	/// particle_system.apply_description(&json!({
	/// 	"looping": false,
	/// 	"duration": 1.2,
	/// 	"emissionRate": 0,
	/// 	"bursts": [{ "time": 0, "count": 25 }],
	/// 	"shape": "circle",
	/// 	"shapeRadius": 6,
	/// 	"lifetime": [0.8, 1.0],
	/// 	"speed": [100, 200],
	/// 	"startColorA": [1, 0.8, 0.2, 1],
	/// 	"blendMode": "lighter",
	/// }));
	/// particle_system.simulate(0.1);
	/// assert_eq!(particle_system.get_particle_count(), 25);
	/// ```
	//==============================================================================
	pub fn apply_description(&mut self, description: &serde_json::Value) {
		let read_number = |name: &str| description.get(name).and_then(|value| value.as_f64()).map(|value| value as f32);
		let read_pair = |name: &str| -> Option<(f32, f32)> {
			let array = description.get(name)?.as_array()?;
			let first = array.first()?.as_f64()? as f32;
			let second = array.get(1).and_then(|value| value.as_f64()).map(|value| value as f32).unwrap_or(first);
			Some((first, second))
		};
		let shape_name = description.get("shape").and_then(|value| value.as_str());
		if let Some(is_looping) = description.get("looping").and_then(|value| value.as_bool()) {
			self.set_looping(is_looping);
		}
		if let Some(duration) = read_number("duration") {
			self.set_duration(duration);
		}
		if let Some(max_particle_count) = read_number("maxParticleCount") {
			self.set_max_particle_count(max_particle_count.max(0.0) as usize);
		}
		if let Some(emission_rate) = read_number("emissionRate") {
			self.set_emission_rate(emission_rate);
		}
		if let Some(burst_array) = description.get("bursts").and_then(|value| value.as_array()) {
			let burst_list = burst_array.iter().map(|burst| {
				let time = burst.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				let count = burst.get("count").and_then(|value| value.as_f64()).unwrap_or(0.0).max(0.0) as usize;
				(time, count)
			}).collect();
			self.set_burst_list(burst_list);
		}
		if let Some(shape_name) = shape_name {
			if let Some((emitter_shape, _)) = EMITTER_SHAPE_NAME_TABLE.iter().find(|(_, name)| *name == shape_name) {
				self.set_emitter_shape(*emitter_shape);
			}
		}
		if let Some(shape_radius) = read_number("shapeRadius") {
			self.set_shape_radius(shape_radius);
		}
		if let Some(cone_angle) = read_number("coneAngle") {
			self.set_cone_angle(cone_angle);
		}
		if let Some((width, height)) = read_pair("boxSize") {
			self.set_box_size(width, height);
		}
		if let Some(edge_width) = read_number("edgeWidth") {
			self.set_edge_width(edge_width);
		}
		if let Some((minimum, maximum)) = read_pair("lifetime") {
			self.set_start_lifetime(minimum, maximum);
		}
		if let Some((minimum, maximum)) = read_pair("speed") {
			self.set_start_speed(minimum, maximum);
		}
		if let Some((minimum, maximum)) = read_pair("size") {
			self.set_start_size(minimum, maximum);
		}
		if let Some((minimum, maximum)) = read_pair("rotation") {
			self.set_start_rotation(minimum, maximum);
		}
		if let Some((minimum, maximum)) = read_pair("angularVelocity") {
			self.set_angular_velocity(minimum, maximum);
		}
		if let Some(color_a) = description.get("startColorA").and_then(read_description_color) {
			let color_b = description.get("startColorB").and_then(read_description_color).unwrap_or(color_a);
			self.set_start_color(color_a, color_b);
		}
		if let Some(key_array) = description.get("colorOverLifetime").and_then(|value| value.as_array()) {
			let gradient = key_array.iter().filter_map(|key| {
				let time = key.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				let color = key.get("color").and_then(read_description_color)?;
				Some((time, color))
			}).collect();
			self.set_color_over_lifetime(gradient);
		}
		if let Some((start_scale, end_scale)) = read_pair("sizeOverLifetime") {
			self.set_size_over_lifetime(start_scale, end_scale);
		}
		if let Some((gravity_x, gravity_y)) = read_pair("gravity") {
			self.set_gravity(gravity_x, gravity_y);
		}
		if let Some(damping) = read_number("damping") {
			self.set_damping(damping);
		}
		match description.get("attractor") {
			Some(serde_json::Value::Object(attractor)) => {
				let read_attractor_number = |name: &str| attractor.get(name).and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				self.set_attractor(read_attractor_number("x"), read_attractor_number("y"), read_attractor_number("strength"), read_attractor_number("swirl"));
			}
			Some(serde_json::Value::Null) | Some(serde_json::Value::Bool(false)) => {
				self.clear_attractor();
			}
			_ => {
			}
		}
		if let Some(wobble_array) = description.get("wobble").and_then(|value| value.as_array()) {
			let amplitude = wobble_array.first().and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
			let frequency = wobble_array.get(1).and_then(|value| value.as_f64()).unwrap_or(4.0) as f32;
			self.set_wobble(amplitude, frequency);
		}
		if let Some(render_shape_name) = description.get("renderShape").and_then(|value| value.as_str()) {
			if let Some((render_shape, _)) = RENDER_SHAPE_NAME_TABLE.iter().find(|(_, name)| *name == render_shape_name) {
				self.set_render_shape(*render_shape);
			}
		}
		if let Some(blend_mode) = description.get("blendMode").and_then(|value| value.as_str()).and_then(BlendMode::from_name) {
			self.set_blend_mode(blend_mode);
		}
		if let Some(streak_scale) = read_number("streakScale") {
			self.set_streak_scale(streak_scale);
		}
		if let Some(is_world_space) = description.get("worldSpace").and_then(|value| value.as_bool()) {
			self.set_world_space(is_world_space);
		}
	}

	//==============================================================================
	/// 살아 있는 입자 수 반환.
	///
	/// # 돌려줌
	/// 살아 있는 입자 수.
	//==============================================================================
	pub fn get_particle_count(&self) -> usize {
		self.particle_list.len()
	}

	//==============================================================================
	/// 입자 자리 목록. (컴포넌트 좌표, 흔들림을 넣지 않은 적분 자리. 같은 씨앗 비교와 시험용)
	///
	/// # 돌려줌
	/// 살아 있는 입자의 자리 목록. (낸 차례)
	//==============================================================================
	pub fn get_particle_position_list(&self) -> Vec<Vector2> {
		self.particle_list.iter().map(|particle| Vector2::create(particle.x, particle.y)).collect()
	}

	//==============================================================================
	/// 재생 중 여부 반환.
	///
	/// # 돌려줌
	/// play 뒤 stop 전이면 true. (기본 true)
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	/// 다 끝났는지. (방출할 것이 더 없고 남은 입자도 없음, 일회성 노드를 치울 때)
	/// - 정지했거나, 루프가 아니고 duration 이 지났으며 버스트를 다 냈으면 방출할 것이 없다.
	///
	/// # 돌려줌
	/// 남은 입자가 없고 더 낼 것도 없으면 true. (반복이면 정지 전까지 false)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		if !self.particle_list.is_empty() {
			return false;
		}
		if !self.is_playing {
			return true;
		}
		!self.is_looping && self.play_elapsed_seconds > self.duration && self.burst_list.iter().all(|burst| burst.is_fired)
	}

	//==============================================================================
	/// 수동 tick 설정. (켜면 노드 갱신에서 진행하지 않고 simulate 로만 진행한다. 타임라인처럼 밖에서 시간을 다룰 때)
	///
	/// # 인자
	/// - `is_manual_tick`: 참이면 노드 tick 은 노드 위치만 읽어 두고 진행은 simulate 를 부를 때만 한다. (기본 거짓)
	//==============================================================================
	pub fn set_manual_tick(&mut self, is_manual_tick: bool) {
		self.is_manual_tick = is_manual_tick;
	}

	//==============================================================================
	/// 수동 tick 여부 반환.
	///
	/// # 돌려줌
	/// 수동 tick 이면 true.
	//==============================================================================
	pub fn is_manual_tick(&self) -> bool {
		self.is_manual_tick
	}

	//==============================================================================
	/// 난수 씨앗 설정. (설정하면 스폰 난수가 씨앗 난수로 바뀌고 stop(true) 마다 씨앗으로 되돌아간다. 되감기 재현용)
	///
	/// # 인자
	/// - `random_seed`: 씨앗 값. (같은 씨앗이면 같은 입자가 나온다)
	///
	/// # 예제
	/// 같은 씨앗의 두 시스템은 같은 자리에 낸다
	/// ```rust
	/// use vanilla::ParticleSystem;
	///
	/// let mut first_system = ParticleSystem::new();
	/// let mut second_system = ParticleSystem::new();
	/// first_system.set_random_seed(42);
	/// second_system.set_random_seed(42);
	/// first_system.emit(8, None);
	/// second_system.emit(8, None);
	/// first_system.simulate(0.25);
	/// second_system.simulate(0.25);
	/// assert_eq!(first_system.get_particle_position_list(), second_system.get_particle_position_list());
	/// assert_eq!(first_system.get_random_seed(), Some(42));
	/// ```
	//==============================================================================
	pub fn set_random_seed(&mut self, random_seed: u32) {
		self.random_seed = Some(random_seed);
		self.random = Some(SeededRandom::new(random_seed));
	}

	//==============================================================================
	/// 난수 씨앗 반환.
	///
	/// # 돌려줌
	/// set_random_seed 로 준 씨앗. 주지 않았으면 None. (전역 난수를 쓴다)
	//==============================================================================
	pub fn get_random_seed(&self) -> Option<u32> {
		self.random_seed
	}

	//==============================================================================
	/// 스폰용 난수. (씨앗이 있으면 씨앗 난수, 없으면 전역 난수)
	///
	/// # 인자
	/// - `minimum_value`: 아래 끝. (포함)
	/// - `maximum_value`: 위 끝. (포함하지 않는다)
	///
	/// # 돌려줌
	/// minimum_value 이상 maximum_value 미만의 무작위 수. (씨앗 난수면 수열이 하나 나아간다)
	//==============================================================================
	pub fn random_range(&mut self, minimum_value: f32, maximum_value: f32) -> f32 {
		match self.random.as_mut() {
			Some(random) => {
				random.next_range(minimum_value, maximum_value)
			}
			None => {
				math::random_range(minimum_value, maximum_value)
			}
		}
	}
}

impl Default for ParticleSystem {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ParticleSystem {
		ParticleSystem::new()
	}
}

#[crate::reflect]
impl Component for ParticleSystem {
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
	/// 컴포넌트 종류 이름 반환. ("ParticleSystem")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ParticleSystem"
	}

	//==============================================================================
	/// 갱신. (노드 위치를 읽어 두고, 수동 tick 이면 진행은 건너뛴다)
	//==============================================================================
	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		self.node_position = context.get_transform().get_local_position();
		if self.is_manual_tick {
			return;
		}
		self.simulate(time_delta);
	}

	//==============================================================================
	/// 출력. (정점 색 배치. circle 은 부드러운 원, rect / streak 은 흰 텍스처, image 는 그림 텍스처)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if self.particle_list.is_empty() {
			return;
		}
		let image_texture = match (self.render_shape, &self.image) {
			(ParticleRenderShape::Image, Some(image)) => {
				match graphic.get_image_texture(image) {
					Some(texture) => {
						Some(texture)
					}
					None => {
						return;
					}
				}
			}
			_ => {
				None
			}
		};
		let is_image_drawn = image_texture.is_some();
		let texture = match image_texture {
			Some(texture) => {
				texture
			}
			None => {
				if self.render_shape == ParticleRenderShape::Circle {
					graphic.get_soft_disc_texture()
				}
				else {
					graphic.get_white_texture()
				}
			}
		};

		// 월드 공간이면 노드 이동을 상쇄해 방출 시점 위치에 고정한다.
		let mut base_offset_x = 0.0;
		let mut base_offset_y = 0.0;
		if self.is_world_space {
			let node_position = transform.get_local_position();
			base_offset_x = -node_position.x;
			base_offset_y = -node_position.y;
		}

		let particle_count = self.particle_list.len();
		let mut vertex_values: Vec<f32> = Vec::with_capacity(particle_count * QUAD_VERTEX_COUNT * PARTICLE_FLOATS_PER_VERTEX);
		for particle in self.particle_list.iter() {
			let life_ratio = particle.age / particle.lifetime;
			let size_scale = self.size_over_lifetime_start + (self.size_over_lifetime_end - self.size_over_lifetime_start) * life_ratio;
			let draw_size = (particle.size * size_scale).max(0.1);
			let half_size = draw_size * 0.5;
			let mut draw_x = particle.x + base_offset_x;
			let draw_y = particle.y + base_offset_y;
			if self.wobble_amplitude > 0.0 {
				draw_x += (particle.age * self.wobble_frequency + particle.wobble_phase).sin() * self.wobble_amplitude;
			}

			// 그림은 원본처럼 색 곡선 없이 시작 알파 x (1 - 수명 비율) 로만 흐린다.
			let color = if is_image_drawn {
				Color::create(1.0, 1.0, 1.0, math::clamp01(particle.alpha * (1.0 - life_ratio)))
			}
			else {
				self.evaluate_color(particle, life_ratio)
			};

			let corners: [(f32, f32); 4] = if self.render_shape == ParticleRenderShape::Streak {
				// 속도 방향으로 길게 늘인 쿼드. (비, 유성)
				let speed = (particle.velocity_x * particle.velocity_x + particle.velocity_y * particle.velocity_y).sqrt();
				let safe_speed = speed.max(0.001);
				let direction_x = particle.velocity_x / safe_speed;
				let direction_y = particle.velocity_y / safe_speed;
				let tail_x = draw_x - particle.velocity_x * self.streak_scale;
				let tail_y = draw_y - particle.velocity_y * self.streak_scale;
				let side_x = -direction_y * half_size;
				let side_y = direction_x * half_size;
				[(draw_x + side_x, draw_y + side_y), (draw_x - side_x, draw_y - side_y), (tail_x - side_x, tail_y - side_y), (tail_x + side_x, tail_y + side_y)]
			}
			else if (self.render_shape == ParticleRenderShape::Rect && particle.rotation != 0.0) || is_image_drawn {
				// 회전 사각형. (그림도 원본처럼 입자의 회전으로 돌린다)
				let cosine = particle.rotation.cos();
				let sine = particle.rotation.sin();
				let axis_x1 = cosine * half_size;
				let axis_y1 = sine * half_size;
				let axis_x2 = -sine * half_size;
				let axis_y2 = cosine * half_size;
				[
					(draw_x - axis_x1 - axis_x2, draw_y - axis_y1 - axis_y2),
					(draw_x + axis_x1 - axis_x2, draw_y + axis_y1 - axis_y2),
					(draw_x + axis_x1 + axis_x2, draw_y + axis_y1 + axis_y2),
					(draw_x - axis_x1 + axis_x2, draw_y - axis_y1 + axis_y2),
				]
			}
			else {
				// 축 정렬 쿼드. (circle 은 부드러운 원 텍스처가 모양을 만든다)
				[(draw_x - half_size, draw_y - half_size), (draw_x + half_size, draw_y - half_size), (draw_x + half_size, draw_y + half_size), (draw_x - half_size, draw_y + half_size)]
			};
			push_quad(&mut vertex_values, corners, color);
		}

		let previous_blend_mode = graphic.get_blend_mode();
		graphic.set_blend_mode(self.blend_mode);
		draw_colored_quads_in_chunks(graphic, &vertex_values, texture);
		graphic.set_blend_mode(previous_blend_mode);
	}
}

//==============================================================================
/// 쿼드 한 장 넣기. (모서리 넷은 왼쪽 위, 오른쪽 위, 오른쪽 아래, 왼쪽 아래 차례. 삼각형 둘 = 정점 여섯, 정점마다 색)
//==============================================================================
fn push_quad(vertex_values: &mut Vec<f32>, corners: [(f32, f32); 4], color: Color) {
	for corner_index in [0usize, 1, 2, 0, 2, 3] {
		let (corner_x, corner_y) = corners[corner_index];
		let (corner_u, corner_v) = QUAD_TEXTURE_COORDINATES[corner_index];
		vertex_values.extend_from_slice(&[corner_x, corner_y, corner_u, corner_v, color.red, color.green, color.blue, color.alpha]);
	}
}

//==============================================================================
/// 정점 색 쿼드를 Graphic 이 한 번에 받는 만큼씩 나눠 그린다. (쿼드가 둘로 갈리지 않게 쿼드 단위로 끊는다)
//==============================================================================
pub(crate) fn draw_colored_quads_in_chunks(graphic: &mut Graphic, vertex_values: &[f32], texture: crate::core::graphic::GlTexture) {
	let quad_float_count = QUAD_VERTEX_COUNT * PARTICLE_FLOATS_PER_VERTEX;
	let mut float_offset = 0;
	while float_offset < vertex_values.len() {
		let remaining_float_count = vertex_values.len() - float_offset;
		let particle_vertex_data = graphic.get_particle_vertex_data(remaining_float_count);
		let chunk_float_count = (particle_vertex_data.len() / quad_float_count) * quad_float_count;
		if chunk_float_count == 0 {
			return;
		}
		particle_vertex_data[..chunk_float_count].copy_from_slice(&vertex_values[float_offset..float_offset + chunk_float_count]);
		graphic.draw_colored_quads(chunk_float_count / PARTICLE_FLOATS_PER_VERTEX, texture);
		float_offset += chunk_float_count;
	}
}

//==============================================================================
/// 서술의 색 배열 읽기. ([r, g, b] 또는 [r, g, b, a], 원본 toColor)
//==============================================================================
fn read_description_color(value: &serde_json::Value) -> Option<Color> {
	let channels = value.as_array()?;
	let read_channel = |channel_index: usize, default_value: f32| channels.get(channel_index).and_then(|channel| channel.as_f64()).map(|channel| channel as f32).unwrap_or(default_value);
	Some(Color::create(read_channel(0, 0.0), read_channel(1, 0.0), read_channel(2, 0.0), read_channel(3, 1.0)))
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// edge 는 가로 선분 위에서 아래(+Y)로 낸다. (원점 보정만큼 옮긴 자리)
	//==============================================================================
	#[test]
	fn edge_shape_emits_downward_on_line() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(7);
		particle_system.set_emitter_shape(ParticleEmitterShape::Edge);
		particle_system.set_edge_width(200.0);
		particle_system.emit(50, Some(Vector2::create(100.0, 20.0)));
		assert_eq!(particle_system.get_particle_count(), 50);
		for particle in particle_system.particle_list.iter() {
			assert!(particle.x >= 0.0 && particle.x <= 200.0);
			assert_eq!(particle.y, 20.0);
			assert!(particle.velocity_y > 0.0);
			assert!(particle.velocity_x.abs() < 0.001);
		}
	}

	//==============================================================================
	/// box 는 사각 영역 안에서 위(-Y)로만 낸다. cone 은 반각 안 방향의 반지름 안에서 낸다.
	//==============================================================================
	#[test]
	fn box_and_cone_follow_original_directions() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(11);
		particle_system.set_emitter_shape(ParticleEmitterShape::Box);
		particle_system.set_box_size(80.0, 40.0);
		particle_system.emit(40, None);
		for particle in particle_system.particle_list.iter() {
			assert!(particle.x.abs() <= 40.0 && particle.y.abs() <= 20.0);
			assert_eq!(particle.velocity_x, 0.0);
			assert_eq!(particle.velocity_y, -60.0);
		}

		particle_system.stop(true);
		particle_system.set_emitter_shape(ParticleEmitterShape::Cone);
		particle_system.set_shape_radius(30.0);
		particle_system.emit(40, None);
		let mut is_any_away_from_origin = false;
		for particle in particle_system.particle_list.iter() {
			let distance = (particle.x * particle.x + particle.y * particle.y).sqrt();
			assert!(distance <= 30.001);
			if distance > 1.0 {
				is_any_away_from_origin = true;
			}
			let angle_from_up = particle.velocity_x.atan2(-particle.velocity_y);
			assert!(angle_from_up.abs() <= 0.5001);
		}
		assert!(is_any_away_from_origin);
	}

	//==============================================================================
	/// 끌개는 그 자리 쪽으로 당기고, 소용돌이는 그 접선 쪽으로 민다.
	//==============================================================================
	#[test]
	fn attractor_pulls_and_swirls() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(3);
		particle_system.set_emission_rate(0.0);
		particle_system.set_start_speed(0.0, 0.0);
		particle_system.set_start_lifetime(10.0, 10.0);
		particle_system.set_emitter_shape(ParticleEmitterShape::Edge);
		particle_system.set_edge_width(0.0);
		particle_system.emit(1, Some(Vector2::create(50.0, 50.0)));
		particle_system.set_attractor(150.0, 50.0, 200.0, 0.0);
		particle_system.simulate(0.1);
		let pulled_velocity_x = particle_system.particle_list[0].velocity_x;
		let pulled_velocity_y = particle_system.particle_list[0].velocity_y;
		assert!(pulled_velocity_x > 0.0);
		assert!(pulled_velocity_y.abs() < 0.001);

		particle_system.stop(true);
		particle_system.emit(1, Some(Vector2::create(50.0, 50.0)));
		particle_system.set_attractor(150.0, 50.0, 0.0, 300.0);
		particle_system.simulate(0.1);
		let swirled_velocity_x = particle_system.particle_list[0].velocity_x;
		let swirled_velocity_y = particle_system.particle_list[0].velocity_y;
		assert!(swirled_velocity_x.abs() < 0.001);
		assert!(swirled_velocity_y > 0.0);

		particle_system.clear_attractor();
		particle_system.simulate(0.1);
		assert_eq!((particle_system.particle_list[0].velocity_x, particle_system.particle_list[0].velocity_y), (swirled_velocity_x, swirled_velocity_y));
	}

	//==============================================================================
	/// 월드 공간이면 낼 때 노드의 로컬 위치를 더해 둔다.
	//==============================================================================
	#[test]
	fn world_space_adds_node_position_on_spawn() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_emitter_shape(ParticleEmitterShape::Edge);
		particle_system.set_edge_width(0.0);
		particle_system.node_position = Vector2::create(300.0, 40.0);
		particle_system.emit(1, None);
		assert_eq!(particle_system.get_particle_position_list()[0], Vector2::zero());
		particle_system.set_world_space(true);
		particle_system.emit(1, None);
		assert_eq!(particle_system.get_particle_position_list()[1], Vector2::create(300.0, 40.0));

		// 트리에서 내면 tick 을 기다리지 않고 지금 노드 위치로 낸다.
		let mut node_tree = NodeTree::new();
		let root_id = node_tree.get_root_id();
		let emitter_id = node_tree.create_child(root_id, "emitter");
		let tree_particle_system = node_tree.add_component(emitter_id, ParticleSystem::new()).expect("particle system");
		tree_particle_system.set_emitter_shape(ParticleEmitterShape::Edge);
		tree_particle_system.set_edge_width(0.0);
		tree_particle_system.set_world_space(true);
		node_tree[emitter_id].set_local_position(Vector2::create(120.0, 80.0));
		assert!(ParticleSystem::emit_in_tree(&mut node_tree, emitter_id, 1, None));
		assert_eq!(node_tree[emitter_id].get_component::<ParticleSystem>().expect("particle system").get_particle_position_list()[0], Vector2::create(120.0, 80.0));
		assert!(!ParticleSystem::emit_in_tree(&mut node_tree, root_id, 1, None));
	}

	//==============================================================================
	/// 루프가 아니면 duration 뒤 방출을 멈추고(원본처럼 재생 중으로 남는다), 버스트를 다 내고 입자가 다 사라지면 끝난 것이다.
	//==============================================================================
	#[test]
	fn finishes_after_duration_without_looping() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(5);
		particle_system.set_looping(false);
		particle_system.set_duration(0.5);
		particle_system.set_emission_rate(0.0);
		particle_system.set_start_lifetime(0.3, 0.3);
		particle_system.set_burst_list(vec![(0.2, 10)]);
		assert!(!particle_system.is_finished());
		for _ in 0..5 {
			particle_system.simulate(0.05);
		}
		assert_eq!(particle_system.get_particle_count(), 10);
		for _ in 0..17 {
			particle_system.simulate(0.05);
		}
		assert!(particle_system.is_playing());
		assert_eq!(particle_system.get_particle_count(), 0);
		assert!(particle_system.is_finished());
	}

	//==============================================================================
	/// 같은 씨앗이면 원본(vanilla.js)과 같은 입자 자리가 나온다. 모양마다 1/64 초씩 90 프레임. (1/64 는 f32 와 f64 에서 같은 값이라 방출 누적이 같은 프레임에 1 을 넘는다)
	/// - 기대값은 원본 effect/particlesystem.js 를 node 로 같은 설정, 같은 씨앗으로 돌려 찍은 값이다. (손 계산이 아니다)
	///   원본은 f64, 러스트 판은 f32 라 오차 한도를 둔다.
	//==============================================================================
	#[test]
	fn same_seed_matches_original_positions() {
		for (emitter_shape, expected_positions) in SAME_SEED_EXPECTED.iter() {
			let mut particle_system = create_same_seed_particle_system(*emitter_shape);
			for _ in 0..90 {
				particle_system.simulate(1.0 / 64.0);
			}
			let position_list = particle_system.get_particle_position_list();
			assert_eq!(position_list.len(), SAME_SEED_EXPECTED_COUNT, "{:?}", emitter_shape);
			for (particle_index, expected_position) in [0usize, 7, SAME_SEED_EXPECTED_COUNT - 1].iter().zip(expected_positions.iter()) {
				let position = position_list[*particle_index];
				assert!((position.x - expected_position.0).abs() < 0.001 && (position.y - expected_position.1).abs() < 0.001, "{:?} {} {:?} {:?}", emitter_shape, particle_index, position, expected_position);
			}
		}
	}

	//==============================================================================
	/// 같은 씨앗 비교 설정. (원본 쪽 시험과 같은 설정)
	//==============================================================================
	fn create_same_seed_particle_system(emitter_shape: ParticleEmitterShape) -> ParticleSystem {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(20261125);
		particle_system.set_emitter_shape(emitter_shape);
		particle_system.set_shape_radius(24.0);
		particle_system.set_cone_angle(0.6);
		particle_system.set_box_size(120.0, 40.0);
		particle_system.set_edge_width(200.0);
		particle_system.set_emission_rate(40.0);
		particle_system.set_burst_list(vec![(0.5, 12)]);
		particle_system.set_start_lifetime(1.2, 2.0);
		particle_system.set_start_speed(40.0, 120.0);
		particle_system.set_start_size(4.0, 9.0);
		particle_system.set_start_rotation(0.0, 3.0);
		particle_system.set_angular_velocity(-2.0, 2.0);
		particle_system.set_gravity(10.0, 90.0);
		particle_system.set_damping(0.4);
		particle_system.set_attractor(30.0, -40.0, 60.0, 30.0);
		particle_system
	}


	/// 원본에서 90 프레임 뒤 살아 있는 입자 수. (모양마다 같다)
	const SAME_SEED_EXPECTED_COUNT: usize = 67;
	/// 원본에서 찍은 0 번, 7 번, 마지막 입자 자리.
	const SAME_SEED_EXPECTED: [(ParticleEmitterShape, [(f32, f32); 3]); 5] = [
		(ParticleEmitterShape::Point, [(84.489852, -7.793926), (6.721317, -1.527093), (-1.519028, 0.371066)]),
		(ParticleEmitterShape::Circle, [(6.347870, 130.951176), (-16.560686, 32.686644), (-11.429110, -21.374512)]),
		(ParticleEmitterShape::Cone, [(33.797671, -13.197808), (44.005587, -21.797820), (4.768423, -22.692181)]),
		(ParticleEmitterShape::Box, [(35.493059, -5.769680), (46.267557, -24.673770), (20.629274, 14.993998)]),
		(ParticleEmitterShape::Edge, [(79.746217, 87.734770), (51.560490, 79.290481), (-7.241791, 1.592986)]),
	];

	//==============================================================================
	/// 서술의 항목을 원본 이름으로 읽는다. (coneAngle 은 원본처럼 반각 그대로)
	//==============================================================================
	#[test]
	fn description_reads_render_and_force_fields() {
		let mut particle_system = ParticleSystem::new();
		particle_system.apply_description(&serde_json::json!({
			"shape": "edge",
			"edgeWidth": 320,
			"coneAngle": 0.25,
			"renderShape": "streak",
			"streakScale": 0.04,
			"wobble": [16, 1.4],
			"attractor": { "x": 10, "y": -20, "strength": 190, "swirl": 260 },
			"worldSpace": true,
		}));
		assert_eq!(particle_system.emitter_shape, ParticleEmitterShape::Edge);
		assert_eq!(particle_system.edge_width, 320.0);
		assert_eq!(particle_system.cone_angle_radian, 0.25);
		assert_eq!(particle_system.render_shape, ParticleRenderShape::Streak);
		assert_eq!(particle_system.streak_scale, 0.04);
		assert_eq!((particle_system.wobble_amplitude, particle_system.wobble_frequency), (16.0, 1.4));
		assert_eq!(particle_system.attractor_position, Some(Vector2::create(10.0, -20.0)));
		assert_eq!((particle_system.attractor_strength, particle_system.attractor_swirl), (190.0, 260.0));
		assert!(particle_system.is_world_space);

		particle_system.apply_description(&serde_json::json!({ "attractor": null, "wobble": [6], "shape": "box" }));
		assert_eq!(particle_system.attractor_position, None);
		assert_eq!((particle_system.wobble_amplitude, particle_system.wobble_frequency), (6.0, 4.0));
		assert_eq!(particle_system.emitter_shape, ParticleEmitterShape::Box);
	}
}
