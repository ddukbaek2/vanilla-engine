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
//!   ```text
//!     원본처럼 낼 때의 노드 위치로 내려면 ParticleSystem::emit_in_tree(트리, 노드, 수, 보정)로 내거나, set_node_position 으로 위치를 넣고 emit 한다.
//!     (Timeline 의 emit 이벤트는 그 자리의 노드 위치를 넣고 낸다)
//!   ```
//!   - 정점 색 쿼드는 한 번에 그릴 수 있는 정점 수(Graphic 한도)씩 나눠 그린다. 원본은 배열을 늘려 한 번에 그린다.
//!   - 서술의 imageDataUrl(파티클 편집기가 고른 그림의 data: 주소)은 데스크탑이 그 자리에서 풀고, 웹은 원본처럼 브라우저가 풀어 다 풀리면 그린다.
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::seededrandom::SeededRandom;
use crate::base::vector2::Vector2;
#[cfg(target_arch = "wasm32")]
use crate::core::asset::AssetLoadHandle;
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
/// 버스트가 이번 시간 구간에 터질 횟수. (원본 countBurstFires)
/// - 구간은 (previous_time, current_time] 이다. 재생 뒤 처음이면 [previous_time, current_time] 이라 0 초의 버스트도 든다.
/// - 루프면 `burst_time + duration * k` (k = 0, 1, 2, ...) 가 모두 버스트 시각이다. (루프 이음매를 건너는 프레임에서도 터지고, 한 프레임에 여러 주기를 건너면 그 수만큼)
///
/// # 인자
/// - `burst_time`: 버스트 시각. (초, 재생 시작부터)
/// - `previous_time`: 지난 프레임의 재생 경과. (초)
/// - `current_time`: 이번 재생 경과. (초)
/// - `is_start_included`: 구간의 시작 시각도 드는지. (재생 뒤 처음)
/// - `loop_duration`: 루프 주기. (초, 0 이하면 루프가 아니라 한 번)
///
/// # 돌려줌
/// 터질 횟수.
//==============================================================================
fn count_burst_fires(burst_time: f32, previous_time: f32, current_time: f32, is_start_included: bool, loop_duration: f32) -> usize {
	if !(loop_duration > 0.0) {
		let is_after_start = if is_start_included {
			burst_time >= previous_time
		}
		else {
			burst_time > previous_time
		};
		if is_after_start && burst_time <= current_time {
			return 1;
		}
		return 0;
	}
	let mut first_cycle = 0.0;
	if !is_start_included {
		first_cycle = (((previous_time - burst_time) / loop_duration).floor() + 1.0).max(0.0);
	}
	let last_cycle = ((current_time - burst_time) / loop_duration).floor();
	(last_cycle - first_cycle + 1.0).max(0.0) as usize
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
	/// 브라우저가 푸는 중인 서술의 그림. (웹, 다 풀리면 그릴 때 image 로 건다)
	#[cfg(target_arch = "wasm32")]
	pending_image_load: Option<AssetLoadHandle>,
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
			#[cfg(target_arch = "wasm32")]
			pending_image_load: None,
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
	//==============================================================================
	pub fn emit(&mut self, count: usize, origin_offset: Option<Vector2>) {
		for _ in 0..count {
			self.spawn_particle(origin_offset);
		}
	}

	//==============================================================================
	/// 노드의 지금 위치로 여러 개 방출. (원본 emit 처럼 낼 때 노드 로컬 위치를 읽는다. 노드나 파티클 시스템이 없으면 false)
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
	//==============================================================================
	pub fn set_node_position(&mut self, node_position: Vector2) {
		self.node_position = node_position;
	}

	//==============================================================================
	/// 시간 진행. (방출 + 적분, 수동 tick 이면 밖에서 부른다)
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

			// 버스트. (이번에 지난 시간 구간에 든 버스트 시각마다 터뜨린다, 루프면 duration 주기로 되풀이한 시각마다, 루프 이음매에서도)
			let loop_duration = if self.is_looping {
				self.duration
			}
			else {
				0.0
			};
			for burst_index in 0..self.burst_list.len() {
				let burst_time = self.burst_list[burst_index].time;
				let is_start_included = previous_elapsed <= 0.0 && !self.burst_list[burst_index].is_fired;
				let fire_count = count_burst_fires(burst_time, previous_elapsed, self.play_elapsed_seconds, is_start_included, loop_duration);
				if fire_count > 0 {
					self.burst_list[burst_index].is_fired = true;
					let burst_count = self.burst_list[burst_index].count;
					self.emit(burst_count * fire_count, None);
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

	pub fn set_emission_rate(&mut self, emission_rate: f32) {
		self.emission_rate = emission_rate;
	}

	//==============================================================================
	/// 버스트 목록 설정. ((재생 시작부터 초, 개수) 목록)
	//==============================================================================
	pub fn set_burst_list(&mut self, burst_list: Vec<(f32, usize)>) {
		self.burst_list = burst_list.into_iter().map(|(time, count)| ParticleBurst { time, count, is_fired: false }).collect();
	}

	pub fn set_emitter_shape(&mut self, emitter_shape: ParticleEmitterShape) {
		self.emitter_shape = emitter_shape;
	}

	//==============================================================================
	/// circle / cone 반지름.
	//==============================================================================
	pub fn set_shape_radius(&mut self, shape_radius: f32) {
		self.shape_radius = shape_radius;
	}

	//==============================================================================
	/// cone 반각. (라디안)
	//==============================================================================
	pub fn set_cone_angle(&mut self, cone_angle_radian: f32) {
		self.cone_angle_radian = cone_angle_radian;
	}

	pub fn set_box_size(&mut self, width: f32, height: f32) {
		self.box_size = Vector2::create(width, height);
	}

	//==============================================================================
	/// edge 방출 모양의 가로 길이.
	//==============================================================================
	pub fn set_edge_width(&mut self, edge_width: f32) {
		self.edge_width = edge_width;
	}

	pub fn set_start_lifetime(&mut self, minimum_seconds: f32, maximum_seconds: f32) {
		self.start_lifetime_min = minimum_seconds;
		self.start_lifetime_max = maximum_seconds;
	}

	pub fn set_start_speed(&mut self, minimum_speed: f32, maximum_speed: f32) {
		self.start_speed_min = minimum_speed;
		self.start_speed_max = maximum_speed;
	}

	pub fn set_start_size(&mut self, minimum_size: f32, maximum_size: f32) {
		self.start_size_min = minimum_size;
		self.start_size_max = maximum_size;
	}

	pub fn set_start_rotation(&mut self, minimum_radian: f32, maximum_radian: f32) {
		self.start_rotation_min = minimum_radian;
		self.start_rotation_max = maximum_radian;
	}

	//==============================================================================
	/// 시작 색. (둘 사이 무작위)
	//==============================================================================
	pub fn set_start_color(&mut self, color_a: impl Into<Color>, color_b: impl Into<Color>) {
		self.start_color_a = color_a.into();
		self.start_color_b = color_b.into();
	}

	//==============================================================================
	/// 수명에 따른 색 곡선. (수명 비율 0 ~ 1 과 색의 쌍 목록, 시작 색에 곱한다)
	//==============================================================================
	pub fn set_color_over_lifetime(&mut self, gradient: Vec<(f32, Color)>) {
		self.color_over_lifetime = gradient;
	}

	pub fn set_size_over_lifetime(&mut self, start_scale: f32, end_scale: f32) {
		self.size_over_lifetime_start = start_scale;
		self.size_over_lifetime_end = end_scale;
	}

	pub fn set_angular_velocity(&mut self, minimum_radian_per_second: f32, maximum_radian_per_second: f32) {
		self.angular_velocity_min = minimum_radian_per_second;
		self.angular_velocity_max = maximum_radian_per_second;
	}

	pub fn set_gravity(&mut self, gravity_x: f32, gravity_y: f32) {
		self.gravity = Vector2::create(gravity_x, gravity_y);
	}

	//==============================================================================
	/// 초당 감쇠 계수. (0 = 없음)
	//==============================================================================
	pub fn set_damping(&mut self, damping: f32) {
		self.damping = damping;
	}

	//==============================================================================
	/// 렌더 모양 설정. (기본 circle)
	//==============================================================================
	pub fn set_render_shape(&mut self, render_shape: ParticleRenderShape) {
		self.render_shape = render_shape;
	}

	//==============================================================================
	/// 입자 이미지. (렌더 모양이 image 일 때 쓴다)
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		#[cfg(target_arch = "wasm32")]
		{
			self.pending_image_load = None;
		}
		self.image = image;
	}

	//==============================================================================
	/// 그림 주소로 입자 이미지 걸기. (서술의 imageDataUrl, 원본 applyDescription 이 Image 의 src 로 넣는 자리)
	/// - 데스크탑은 data: 주소를 그 자리에서 풀고, 풀지 못하면 그림을 비운다.
	/// - 웹은 원본처럼 브라우저가 풀게 하고 그동안은 그림 없이 둔다. 다 풀리면 그릴 때 건다.
	///
	/// # 인자
	/// - `image_data_url`: 그림의 data: 주소.
	//==============================================================================
	fn load_image_data_url(&mut self, image_data_url: &str) {
		#[cfg(not(target_arch = "wasm32"))]
		{
			let image = crate::base::dataurl::parse_data_url(image_data_url).and_then(|data_url| Image::load_from_memory_with_mime_type(&data_url.bytes, &data_url.mime_type).ok());
			self.set_image(image);
		}
		#[cfg(target_arch = "wasm32")]
		{
			self.set_image(None);
			let load_handle = AssetLoadHandle::create_pending(image_data_url);
			crate::platform::request_asset_bytes(image_data_url.to_string(), load_handle.clone(), false);
			self.pending_image_load = Some(load_handle);
		}
	}

	//==============================================================================
	/// 브라우저가 다 푼 서술의 그림 걸기. (웹, 못 풀었으면 그림 없이 둔다)
	//==============================================================================
	#[cfg(target_arch = "wasm32")]
	fn apply_pending_image(&mut self) {
		let is_finished = self.pending_image_load.as_ref().map(|load_handle| load_handle.is_finished()).unwrap_or(false);
		if !is_finished {
			return;
		}
		if let Some(load_handle) = self.pending_image_load.take() {
			self.image = load_handle.get_decoded_image();
		}
	}

	//==============================================================================
	/// 더하기 합성 여부. (set_blend_mode(Lighter) 의 설탕)
	//==============================================================================
	pub fn set_additive(&mut self, is_additive: bool) {
		self.blend_mode = if is_additive {
			BlendMode::Lighter
		}
		else {
			BlendMode::Normal
		};
	}

	pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
		self.blend_mode = blend_mode;
	}

	//==============================================================================
	/// 끌개 설정. (컴포넌트 좌표의 자리, 중심 쪽 가속(음수면 밀어냄), 접선 쪽 가속(소용돌이))
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
	//==============================================================================
	pub fn set_wobble(&mut self, amplitude: f32, frequency: f32) {
		self.wobble_amplitude = amplitude;
		self.wobble_frequency = frequency;
	}

	//==============================================================================
	/// streak 길이 비율. (띠의 길이 = 속도 x 이 값)
	//==============================================================================
	pub fn set_streak_scale(&mut self, streak_scale: f32) {
		self.streak_scale = streak_scale;
	}

	//==============================================================================
	/// 월드 공간 설정. (켜면 낸 뒤 노드가 움직여도 입자는 그 자리에 남는다)
	//==============================================================================
	pub fn set_world_space(&mut self, is_world_space: bool) {
		self.is_world_space = is_world_space;
	}

	pub fn set_looping(&mut self, is_looping: bool) {
		self.is_looping = is_looping;
	}

	//==============================================================================
	/// 루프 꺼짐일 때 방출 시간. (버스트 반복 주기)
	//==============================================================================
	pub fn set_duration(&mut self, duration: f32) {
		self.duration = duration;
	}

	pub fn set_max_particle_count(&mut self, max_particle_count: usize) {
		self.max_particle_count = max_particle_count;
	}

	//==============================================================================
	/// 서술(JSON) 적용. (원본 applyDescription. vfx 애셋 로드, 파티클 편집기 공용. 준 항목만 반영한다. 색은 [r, g, b, a] 배열)
	/// - attractor 는 { x, y, strength, swirl } 이고 null 이면 끌개를 푼다. wobble 은 [폭, 각속도] 이고 각속도를 빼면 4 다.
	/// - imageDataUrl 은 파티클 편집기가 고른 그림의 data: 주소다. 그 그림을 입자 이미지로 걸고, 비었거나 null 이면 그림을 비운다.
	/// - [최소, 최대] 쌍에서 최대를 빼면 원본 기본 인자처럼 최소와 같다.
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
		// 파티클 편집기가 고른 그림. (비었거나 글이 아니면 그림을 비운다)
		if let Some(image_data_url) = description.get("imageDataUrl") {
			match image_data_url.as_str() {
				Some(image_data_url) if !image_data_url.is_empty() => {
					self.load_image_data_url(image_data_url);
				}
				_ => {
					self.set_image(None);
				}
			}
		}
	}

	pub fn get_particle_count(&self) -> usize {
		self.particle_list.len()
	}

	//==============================================================================
	/// 입자 자리 목록. (컴포넌트 좌표, 흔들림을 넣지 않은 적분 자리. 같은 씨앗 비교와 시험용)
	//==============================================================================
	pub fn get_particle_position_list(&self) -> Vec<Vector2> {
		self.particle_list.iter().map(|particle| Vector2::create(particle.x, particle.y)).collect()
	}

	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	/// 다 끝났는지. (방출할 것이 더 없고 남은 입자도 없음, 일회성 노드를 치울 때)
	/// - 정지했거나, 루프가 아니고 duration 이 지났으며 버스트를 다 냈으면 방출할 것이 없다.
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
	//==============================================================================
	pub fn set_manual_tick(&mut self, is_manual_tick: bool) {
		self.is_manual_tick = is_manual_tick;
	}

	pub fn is_manual_tick(&self) -> bool {
		self.is_manual_tick
	}

	//==============================================================================
	/// 난수 씨앗 설정. (설정하면 스폰 난수가 씨앗 난수로 바뀌고 stop(true) 마다 씨앗으로 되돌아간다. 되감기 재현용)
	//==============================================================================
	pub fn set_random_seed(&mut self, random_seed: u32) {
		self.random_seed = Some(random_seed);
		self.random = Some(SeededRandom::new(random_seed));
	}

	pub fn get_random_seed(&self) -> Option<u32> {
		self.random_seed
	}

	//==============================================================================
	/// 스폰용 난수. (씨앗이 있으면 씨앗 난수, 없으면 전역 난수)
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
	fn default() -> ParticleSystem {
		ParticleSystem::new()
	}
}

#[crate::reflect]
impl Component for ParticleSystem {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

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
		#[cfg(target_arch = "wasm32")]
		{
			self.apply_pending_image();
			// 원본처럼 그림이 다 풀리기 전에는 image 입자를 그리지 않는다. (풀리기 전의 Image 는 크기가 0 이라 그려지지 않는다)
			if self.render_shape == ParticleRenderShape::Image && self.pending_image_load.is_some() {
				return;
			}
		}
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

	//==============================================================================
	/// 루프 이음매(되풀이 경계)의 버스트가 첫 주기만이 아니라 주기마다 터지는지. (0 초 버스트와 경계 바로 앞 버스트)
	//==============================================================================
	#[test]
	fn looping_burst_fires_every_cycle_at_seam() {
		let mut particle_system = ParticleSystem::new();
		particle_system.set_random_seed(3);
		particle_system.set_looping(true);
		particle_system.set_duration(1.0);
		particle_system.set_emission_rate(0.0);
		particle_system.set_start_lifetime(100.0, 100.0);
		particle_system.set_burst_list(vec![(0.0, 2), (0.98, 1)]);
		// 0.25 초씩 3 초. (0 초 버스트는 0, 1, 2, 3 초에, 0.98 초 버스트는 0.98, 1.98, 2.98 초에)
		for _ in 0..12 {
			particle_system.simulate(0.25);
		}
		assert_eq!(particle_system.get_particle_count(), 2 * 4 + 3);

		// 한 프레임에 여러 주기를 건너면 그 수만큼.
		let mut long_frame_particle_system = ParticleSystem::new();
		long_frame_particle_system.set_random_seed(3);
		long_frame_particle_system.set_looping(true);
		long_frame_particle_system.set_duration(1.0);
		long_frame_particle_system.set_emission_rate(0.0);
		long_frame_particle_system.set_start_lifetime(100.0, 100.0);
		long_frame_particle_system.set_burst_list(vec![(0.5, 1)]);
		long_frame_particle_system.simulate(3.2);
		assert_eq!(long_frame_particle_system.get_particle_count(), 3);
	}

	//==============================================================================
	/// 버스트 구간 셈: 재생 뒤 처음만 시작 시각이 들고, 시간이 멈춘 프레임(0 초)에는 다시 터지지 않는다.
	//==============================================================================
	#[test]
	fn burst_fire_count_uses_half_open_window() {
		assert_eq!(count_burst_fires(0.0, 0.0, 0.0, true, 1.0), 1);
		assert_eq!(count_burst_fires(0.0, 0.0, 0.0, false, 1.0), 0);
		assert_eq!(count_burst_fires(0.0, 0.75, 1.0, false, 1.0), 1);
		assert_eq!(count_burst_fires(0.0, 1.0, 1.25, false, 1.0), 0);
		assert_eq!(count_burst_fires(0.5, 0.25, 0.5, false, 0.0), 1);
		assert_eq!(count_burst_fires(0.5, 0.5, 0.75, false, 0.0), 0);
		assert_eq!(count_burst_fires(0.0, 0.0, 0.1, true, 0.0), 1);
	}
	//==============================================================================
	/// 서술의 imageDataUrl(파티클 편집기가 넣는 그림)을 읽어 입자 이미지로 거는지. (비우면 그림을 뗀다)
	//==============================================================================
	#[test]
	fn description_reads_image_data_url() {
		let png_base64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
		let mut particle_system = ParticleSystem::new();
		particle_system.apply_description(&serde_json::json!({
			"renderShape": "image",
			"imageDataUrl": format!("data:image/png;base64,{}", png_base64),
		}));
		assert_eq!(particle_system.render_shape, ParticleRenderShape::Image);
		let image = particle_system.image.as_ref().expect("image");
		assert_eq!((image.get_width(), image.get_height()), (1, 1));
		particle_system.apply_description(&serde_json::json!({ "emissionRate": 5 }));
		assert!(particle_system.image.is_some());
		particle_system.apply_description(&serde_json::json!({ "imageDataUrl": null }));
		assert!(particle_system.image.is_none());
	}
}
