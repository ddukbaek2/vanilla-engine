//==============================================================================
// 파티클 시스템 컴포넌트.
// - 원본 ParticleSystem 의 줄인 판. 이름과 쓰임새(play / stop / emit / setStart...)는 원본을 따른다.
// - 입자는 노드 로컬 좌표(content_size 가운데가 방출점)에서 움직이고, 정점마다 색을 실어 draw_colored_quads 한 번으로 그린다.
// - 원본에서 옮기지 않은 것: 끌개, 흔들림, 늘어짐, 월드 공간, 렌더 모양, edge 방출 모양(서술의 edge 는 폭 edge_width, 높이 0 인 rectangle 로 읽는다).
// - 타임라인이 쓰는 것은 옮겼다: 버스트 목록(set_burst_list), 수동 tick(set_manual_tick + simulate), 씨앗 고정(set_random_seed), 서술 적용(apply_description).
//   simulate 는 원본과 달리 방출점(노드 로컬, 보통 content_size 가운데)을 같이 받는다. (emit 과 같은 까닭: 노드 없이 부른다)
//==============================================================================
use crate::base::color::Color;
use crate::base::math;
use crate::base::seededrandom::SeededRandom;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic, PARTICLE_FLOATS_PER_VERTEX};
use crate::core::image::Image;
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use std::rc::Rc;

//==============================================================================
// 방출 모양.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmitterShape {
	// 한 점에서 모든 방향으로.
	Point,
	// 반지름 안의 무작위 점에서 바깥 방향으로.
	Circle,
	// 위쪽(-Y)을 가운데로 cone_angle 폭 안의 방향으로.
	Cone,
	// box_size 사각형 안의 (원본 이름은 box, 러스트 예약어라 rectangle) 무작위 점에서, 위쪽(-Y)을 가운데로 cone_angle 폭 안의 방향으로.
	Rectangle,
}

// 서술의 방출 모양 이름. (원본 ParticleEmitterShape 값, box 는 rectangle)
const EMITTER_SHAPE_NAME_TABLE: [(EmitterShape, &str); 4] = [
	(EmitterShape::Point, "point"),
	(EmitterShape::Circle, "circle"),
	(EmitterShape::Cone, "cone"),
	(EmitterShape::Rectangle, "box"),
];

//==============================================================================
// 버스트 하나. (재생 시작부터 time 초에 count 개를 한 번에 낸다. 반복이면 duration 주기마다)
//==============================================================================
struct ParticleBurst {
	time: f32,
	count: usize,
	is_fired: bool,
}

struct Particle {
	position: Vector2,
	velocity: Vector2,
	rotation: f32,
	angular_velocity: f32,
	start_size: f32,
	start_color: Color,
	age: f32,
	lifetime: f32,
}

pub struct ParticleSystem {
	component_base: ComponentBase,
	particles: Vec<Particle>,
	is_playing: bool,
	is_looping: bool,
	duration: f32,
	elapsed_time: f32,
	emission_rate: f32,
	emission_accumulator: f32,
	emitter_shape: EmitterShape,
	shape_radius: f32,
	cone_angle: f32,
	box_size: Vector2,
	start_lifetime: (f32, f32),
	start_speed: (f32, f32),
	start_size: (f32, f32),
	start_rotation: (f32, f32),
	start_color: (Color, Color),
	color_over_lifetime: Vec<(f32, Color)>,
	size_over_lifetime: (f32, f32),
	angular_velocity: (f32, f32),
	gravity: Vector2,
	damping: f32,
	image: Option<Rc<Image>>,
	blend_mode: BlendMode,
	max_particle_count: usize,
	burst_list: Vec<ParticleBurst>,
	is_manual_tick: bool,
	random: Option<SeededRandom>,
	random_seed: Option<u32>,
}

impl ParticleSystem {
	//==============================================================================
	// 생성. (재생 중, 반복, 초당 20 개)
	//==============================================================================
	pub fn new() -> ParticleSystem {
		ParticleSystem {
			component_base: ComponentBase::new(),
			particles: Vec::new(),
			is_playing: true,
			is_looping: true,
			duration: 1.0,
			elapsed_time: 0.0,
			emission_rate: 20.0,
			emission_accumulator: 0.0,
			emitter_shape: EmitterShape::Point,
			shape_radius: 0.0,
			cone_angle: math::degree_to_radian(30.0),
			box_size: Vector2::zero(),
			start_lifetime: (1.0, 1.0),
			start_speed: (100.0, 100.0),
			start_size: (16.0, 16.0),
			start_rotation: (0.0, 0.0),
			start_color: (Color::white(), Color::white()),
			color_over_lifetime: Vec::new(),
			size_over_lifetime: (1.0, 1.0),
			angular_velocity: (0.0, 0.0),
			gravity: Vector2::zero(),
			damping: 0.0,
			image: None,
			blend_mode: BlendMode::Normal,
			max_particle_count: 1000,
			burst_list: Vec::new(),
			is_manual_tick: false,
			random: None,
			random_seed: None,
		}
	}

	//==============================================================================
	// 재생. (방출 시작)
	//==============================================================================
	pub fn play(&mut self) {
		self.is_playing = true;
		self.elapsed_time = 0.0;
		self.emission_accumulator = 0.0;
		for burst in self.burst_list.iter_mut() {
			burst.is_fired = false;
		}
	}

	//==============================================================================
	// 정지. (is_clearing 이면 살아 있는 입자도 지운다)
	//==============================================================================
	pub fn stop(&mut self, is_clearing: bool) {
		self.is_playing = false;
		if is_clearing {
			self.particles.clear();
			self.emission_accumulator = 0.0;
			if let (Some(random), Some(random_seed)) = (self.random.as_mut(), self.random_seed) {
				random.set_seed(random_seed);
			}
		}
	}

	//==============================================================================
	// 즉시 방출. (재생 여부와 상관없이 count 개)
	//==============================================================================
	pub fn emit(&mut self, count: usize, emitter_center: Vector2) {
		for _ in 0..count {
			if self.particles.len() >= self.max_particle_count {
				break;
			}
			let particle = self.create_particle(emitter_center);
			self.particles.push(particle);
		}
	}

	//==============================================================================
	// 입자 하나 만들기.
	//==============================================================================
	fn create_particle(&mut self, emitter_center: Vector2) -> Particle {
		let (offset, direction_radian) = match self.emitter_shape {
			EmitterShape::Point => {
				(Vector2::zero(), self.random_range(0.0, math::PI * 2.0))
			}
			EmitterShape::Circle => {
				let radian = self.random_range(0.0, math::PI * 2.0);
				let distance = self.shape_radius * self.random_range(0.0, 1.0).sqrt();
				(Vector2::create(radian.cos() * distance, radian.sin() * distance), radian)
			}
			EmitterShape::Cone => {
				let half_cone_angle = self.cone_angle * 0.5;
				let radian = -math::PI * 0.5 + self.random_range(-half_cone_angle, half_cone_angle);
				(Vector2::zero(), radian)
			}
			EmitterShape::Rectangle => {
				let half_cone_angle = self.cone_angle * 0.5;
				let radian = -math::PI * 0.5 + self.random_range(-half_cone_angle, half_cone_angle);
				let offset_x = self.random_range(-0.5, 0.5) * self.box_size.x;
				let offset_y = self.random_range(-0.5, 0.5) * self.box_size.y;
				(Vector2::create(offset_x, offset_y), radian)
			}
		};
		let speed = self.random_between(self.start_speed);
		let color_mix = self.random_range(0.0, 1.0);
		let rotation = self.random_between(self.start_rotation);
		let angular_velocity = self.random_between(self.angular_velocity);
		let start_size = self.random_between(self.start_size);
		let lifetime = self.random_between(self.start_lifetime).max(0.01);
		Particle {
			position: emitter_center.add(offset),
			velocity: Vector2::create(direction_radian.cos() * speed, direction_radian.sin() * speed),
			rotation,
			angular_velocity,
			start_size,
			start_color: Color::lerp(self.start_color.0, self.start_color.1, color_mix),
			age: 0.0,
			lifetime,
		}
	}

	//==============================================================================
	// 스폰용 난수. (씨앗이 있으면 씨앗 난수, 없으면 전역 난수, 원본 random_range)
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

	//==============================================================================
	// (최소, 최대) 사이 스폰용 난수.
	//==============================================================================
	fn random_between(&mut self, range: (f32, f32)) -> f32 {
		self.random_range(range.0, range.1)
	}

	//==============================================================================
	// 시간 진행. (방출 + 버스트 + 적분. 수동 tick 이면 밖에서 부른다. 방출점은 노드 로컬 좌표)
	//==============================================================================
	pub fn simulate(&mut self, time_delta: f32, emitter_center: Vector2) {
		// 방출.
		if self.is_playing {
			let previous_elapsed_time = self.elapsed_time;
			self.elapsed_time += time_delta;
			let is_emit_window = self.is_looping || self.elapsed_time < self.duration;
			if is_emit_window && self.emission_rate > 0.0 {
				self.emission_accumulator += self.emission_rate * time_delta;
				let emit_count = self.emission_accumulator.floor();
				self.emission_accumulator -= emit_count;
				self.emit(emit_count as usize, emitter_center);
			}
			self.fire_bursts(previous_elapsed_time, emitter_center);
			if !is_emit_window {
				self.is_playing = false;
			}
		}

		// 입자 갱신.
		let gravity = self.gravity;
		let damping_factor = (1.0 - self.damping * time_delta).max(0.0);
		for particle in self.particles.iter_mut() {
			particle.age += time_delta;
			particle.velocity = particle.velocity.add(gravity.multiply_scalar(time_delta)).multiply_scalar(damping_factor);
			particle.position = particle.position.add(particle.velocity.multiply_scalar(time_delta));
			particle.rotation += particle.angular_velocity * time_delta;
		}
		self.particles.retain(|particle| particle.age < particle.lifetime);
	}

	//==============================================================================
	// 버스트 발사. (지난 시간 ~ 지금 사이에 든 버스트, 반복이면 duration 주기로 다시 든다. 원본 simulate 의 버스트 부분)
	//==============================================================================
	fn fire_bursts(&mut self, previous_elapsed_time: f32, emitter_center: Vector2) {
		let mut emit_count_list: Vec<usize> = Vec::new();
		let is_cycling = self.is_looping && self.duration > 0.0;
		for burst in self.burst_list.iter_mut() {
			let mut current_time = self.elapsed_time;
			let mut previous_time = previous_elapsed_time;
			if is_cycling {
				current_time = self.elapsed_time % self.duration;
				previous_time = previous_elapsed_time % self.duration;
				if current_time < previous_time {
					burst.is_fired = false;
				}
			}
			if !burst.is_fired && previous_time <= burst.time && current_time >= burst.time {
				burst.is_fired = true;
				emit_count_list.push(burst.count);
			}
		}
		for emit_count in emit_count_list {
			self.emit(emit_count, emitter_center);
		}
	}

	//==============================================================================
	// 수명 비율에 맞는 색. (색 곡선이 없으면 시작 색)
	//==============================================================================
	fn evaluate_color(&self, start_color: Color, life_ratio: f32) -> Color {
		if self.color_over_lifetime.is_empty() {
			return start_color;
		}
		let first_stop = self.color_over_lifetime[0];
		if life_ratio <= first_stop.0 {
			return multiply_color(start_color, first_stop.1);
		}
		for stop_index in 1..self.color_over_lifetime.len() {
			let previous_stop = self.color_over_lifetime[stop_index - 1];
			let next_stop = self.color_over_lifetime[stop_index];
			if life_ratio <= next_stop.0 {
				let span = (next_stop.0 - previous_stop.0).max(0.0001);
				let stop_ratio = (life_ratio - previous_stop.0) / span;
				let gradient_color = Color::lerp(previous_stop.1, next_stop.1, stop_ratio);
				return multiply_color(start_color, gradient_color);
			}
		}
		let last_stop = self.color_over_lifetime[self.color_over_lifetime.len() - 1];
		multiply_color(start_color, last_stop.1)
	}

	pub fn set_emission_rate(&mut self, emission_rate: f32) {
		self.emission_rate = emission_rate.max(0.0);
	}

	pub fn set_emitter_shape(&mut self, emitter_shape: EmitterShape) {
		self.emitter_shape = emitter_shape;
	}

	pub fn set_shape_radius(&mut self, shape_radius: f32) {
		self.shape_radius = shape_radius.max(0.0);
	}

	pub fn set_cone_angle(&mut self, cone_angle_radian: f32) {
		self.cone_angle = cone_angle_radian;
	}

	pub fn set_box_size(&mut self, width: f32, height: f32) {
		self.box_size = Vector2::create(width, height);
	}

	pub fn set_start_lifetime(&mut self, minimum_seconds: f32, maximum_seconds: f32) {
		self.start_lifetime = (minimum_seconds, maximum_seconds);
	}

	pub fn set_start_speed(&mut self, minimum_speed: f32, maximum_speed: f32) {
		self.start_speed = (minimum_speed, maximum_speed);
	}

	pub fn set_start_size(&mut self, minimum_size: f32, maximum_size: f32) {
		self.start_size = (minimum_size, maximum_size);
	}

	pub fn set_start_rotation(&mut self, minimum_radian: f32, maximum_radian: f32) {
		self.start_rotation = (minimum_radian, maximum_radian);
	}

	pub fn set_start_color(&mut self, color_a: impl Into<Color>, color_b: impl Into<Color>) {
		self.start_color = (color_a.into(), color_b.into());
	}

	//==============================================================================
	// 수명에 따른 색 곡선. (수명 비율 0 ~ 1 과 색의 쌍 목록, 시작 색에 곱한다)
	//==============================================================================
	pub fn set_color_over_lifetime(&mut self, gradient: Vec<(f32, Color)>) {
		self.color_over_lifetime = gradient;
	}

	pub fn set_size_over_lifetime(&mut self, start_scale: f32, end_scale: f32) {
		self.size_over_lifetime = (start_scale, end_scale);
	}

	pub fn set_angular_velocity(&mut self, minimum_radian_per_second: f32, maximum_radian_per_second: f32) {
		self.angular_velocity = (minimum_radian_per_second, maximum_radian_per_second);
	}

	pub fn set_gravity(&mut self, gravity_x: f32, gravity_y: f32) {
		self.gravity = Vector2::create(gravity_x, gravity_y);
	}

	pub fn set_damping(&mut self, damping: f32) {
		self.damping = damping.max(0.0);
	}

	//==============================================================================
	// 입자 이미지. (None 이면 부드러운 원)
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	// 더하기 합성 여부. (켜면 lighter)
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

	pub fn set_looping(&mut self, is_looping: bool) {
		self.is_looping = is_looping;
	}

	pub fn set_duration(&mut self, duration: f32) {
		self.duration = duration.max(0.0);
	}

	pub fn set_max_particle_count(&mut self, max_particle_count: usize) {
		self.max_particle_count = max_particle_count;
	}

	pub fn get_particle_count(&self) -> usize {
		self.particles.len()
	}

	pub fn is_playing(&self) -> bool {
		self.is_playing
	}

	//==============================================================================
	// 다 끝났는지. (방출이 멈췄고 남은 입자도 없음, 일회성 노드를 치울 때)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		!self.is_playing && self.particles.is_empty()
	}

	//==============================================================================
	// 버스트 목록 설정. ((재생 시작부터 초, 개수) 목록)
	//==============================================================================
	pub fn set_burst_list(&mut self, burst_list: Vec<(f32, usize)>) {
		self.burst_list = burst_list.into_iter().map(|(time, count)| ParticleBurst { time, count, is_fired: false }).collect();
	}

	//==============================================================================
	// 수동 tick 설정. (켜면 노드 갱신에서 진행하지 않고 simulate 로만 진행한다. 타임라인처럼 밖에서 시간을 다룰 때)
	//==============================================================================
	pub fn set_manual_tick(&mut self, is_manual_tick: bool) {
		self.is_manual_tick = is_manual_tick;
	}

	pub fn is_manual_tick(&self) -> bool {
		self.is_manual_tick
	}

	//==============================================================================
	// 난수 씨앗 설정. (설정하면 스폰 난수가 씨앗 난수로 바뀌고 stop(true) 마다 씨앗으로 되돌아간다. 되감기 재현용)
	//==============================================================================
	pub fn set_random_seed(&mut self, random_seed: u32) {
		self.random_seed = Some(random_seed);
		self.random = Some(SeededRandom::new(random_seed));
	}

	pub fn get_random_seed(&self) -> Option<u32> {
		self.random_seed
	}

	//==============================================================================
	// 서술(JSON) 적용. (원본 apply_description. 준 항목만 반영한다. 색은 [r, g, b, a] 배열)
	// - 옮기지 않은 항목(attractor, wobble, renderShape, streakScale, worldSpace)은 읽지 않는다.
	// - cone_angle 은 원본처럼 가운데에서 한쪽으로 벌어지는 각이라, 전체 폭을 받는 set_cone_angle 에는 두 배로 넣는다.
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
			self.set_cone_angle(cone_angle * 2.0);
		}
		if let Some((width, height)) = read_pair("boxSize") {
			self.set_box_size(width, height);
		}
		if shape_name == Some("edge") {
			let edge_width = read_number("edgeWidth").unwrap_or(0.0);
			self.set_emitter_shape(EmitterShape::Rectangle);
			self.set_box_size(edge_width, 0.0);
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
		if let Some(blend_mode) = description.get("blendMode").and_then(|value| value.as_str()).and_then(BlendMode::from_name) {
			self.set_blend_mode(blend_mode);
		}
	}
}

impl Default for ParticleSystem {
	fn default() -> ParticleSystem {
		ParticleSystem::new()
	}
}

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

	fn tick(&mut self, context: &mut ComponentContext, time_delta: f32) {
		if self.is_manual_tick {
			return;
		}
		let emitter_center = context.get_transform().get_content_size().multiply_scalar(0.5);
		self.simulate(time_delta, emitter_center);
	}

	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		if self.particles.is_empty() {
			return;
		}
		let texture = match &self.image {
			Some(image) => {
				match graphic.get_image_texture(image) {
					Some(texture) => {
						texture
					}
					None => {
						return;
					}
				}
			}
			None => {
				graphic.get_soft_disc_texture()
			}
		};

		// 입자마다 쿼드 한 장. (회전한 네 모서리 + 정점 색)
		let particle_count = self.particles.len();
		let mut vertex_values: Vec<f32> = Vec::with_capacity(particle_count * 6 * PARTICLE_FLOATS_PER_VERTEX);
		for particle in self.particles.iter() {
			let life_ratio = math::clamp01(particle.age / particle.lifetime);
			let size = particle.start_size * math::lerp(self.size_over_lifetime.0, self.size_over_lifetime.1, life_ratio);
			let color = self.evaluate_color(particle.start_color, life_ratio);
			let half_size = size * 0.5;
			let cosine = particle.rotation.cos() * half_size;
			let sine = particle.rotation.sin() * half_size;
			let center_x = particle.position.x;
			let center_y = particle.position.y;
			let corners: [(f32, f32, f32, f32); 4] = [
				(center_x - cosine + sine, center_y - sine - cosine, 0.0, 0.0),
				(center_x + cosine + sine, center_y + sine - cosine, 1.0, 0.0),
				(center_x - cosine - sine, center_y - sine + cosine, 0.0, 1.0),
				(center_x + cosine - sine, center_y + sine + cosine, 1.0, 1.0),
			];
			for corner_index in [0usize, 1, 2, 2, 1, 3] {
				let (corner_x, corner_y, corner_u, corner_v) = corners[corner_index];
				vertex_values.extend_from_slice(&[corner_x, corner_y, corner_u, corner_v, color.red, color.green, color.blue, color.alpha]);
			}
		}

		let particle_vertex_data = graphic.get_particle_vertex_data(vertex_values.len());
		let writable_float_count = particle_vertex_data.len();
		particle_vertex_data.copy_from_slice(&vertex_values[..writable_float_count]);
		let vertex_count = writable_float_count / PARTICLE_FLOATS_PER_VERTEX;

		let previous_blend_mode = graphic.get_blend_mode();
		graphic.set_blend_mode(self.blend_mode);
		graphic.draw_colored_quads(vertex_count, texture);
		graphic.set_blend_mode(previous_blend_mode);
	}
}

//==============================================================================
// 서술의 색 배열 읽기. ([r, g, b] 또는 [r, g, b, a], 원본 toColor)
//==============================================================================
fn read_description_color(value: &serde_json::Value) -> Option<Color> {
	let channels = value.as_array()?;
	let read_channel = |channel_index: usize, default_value: f32| channels.get(channel_index).and_then(|channel| channel.as_f64()).map(|channel| channel as f32).unwrap_or(default_value);
	Some(Color::create(read_channel(0, 0.0), read_channel(1, 0.0), read_channel(2, 0.0), read_channel(3, 1.0)))
}

//==============================================================================
// 색 성분별 곱.
//==============================================================================
fn multiply_color(color_a: Color, color_b: Color) -> Color {
	Color::create(color_a.red * color_b.red, color_a.green * color_b.green, color_a.blue * color_b.blue, color_a.alpha * color_b.alpha)
}
