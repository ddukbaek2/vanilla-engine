//==============================================================================
// 파티클 시스템 컴포넌트.
// - 원본 ParticleSystem 의 줄인 판. 이름과 쓰임새(play / stop / emit / setStart...)는 원본을 따른다.
// - 입자는 노드 로컬 좌표(contentSize 가운데가 방출점)에서 움직이고, 정점마다 색을 실어 drawColoredQuads 한 번으로 그린다.
// - 원본에서 옮기지 않은 것: 끌개, 흔들림, 늘어짐, 월드 공간, 렌더 모양, edge 방출 모양(서술의 edge 는 폭 edgeWidth, 높이 0 인 rectangle 로 읽는다).
// - 타임라인이 쓰는 것은 옮겼다: 버스트 목록(setBurstList), 수동 tick(setManualTick + simulate), 씨앗 고정(setRandomSeed), 서술 적용(applyDescription).
//   simulate 는 원본과 달리 방출점(노드 로컬, 보통 contentSize 가운데)을 같이 받는다. (emit 과 같은 까닭: 노드 없이 부른다)
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
	point,
	// 반지름 안의 무작위 점에서 바깥 방향으로.
	circle,
	// 위쪽(-Y)을 가운데로 coneAngle 폭 안의 방향으로.
	cone,
	// boxSize 사각형 안의 (원본 이름은 box, 러스트 예약어라 rectangle) 무작위 점에서, 위쪽(-Y)을 가운데로 coneAngle 폭 안의 방향으로.
	rectangle,
}

// 서술의 방출 모양 이름. (원본 ParticleEmitterShape 값, box 는 rectangle)
const EMITTER_SHAPE_NAME_TABLE: [(EmitterShape, &str); 4] = [
	(EmitterShape::point, "point"),
	(EmitterShape::circle, "circle"),
	(EmitterShape::cone, "cone"),
	(EmitterShape::rectangle, "box"),
];

//==============================================================================
// 버스트 하나. (재생 시작부터 time 초에 count 개를 한 번에 낸다. 반복이면 duration 주기마다)
//==============================================================================
struct ParticleBurst {
	time: f32,
	count: usize,
	isFired: bool,
}

struct Particle {
	position: Vector2,
	velocity: Vector2,
	rotation: f32,
	angularVelocity: f32,
	startSize: f32,
	startColor: Color,
	age: f32,
	lifetime: f32,
}

pub struct ParticleSystem {
	componentBase: ComponentBase,
	particles: Vec<Particle>,
	isPlaying: bool,
	isLooping: bool,
	duration: f32,
	elapsedTime: f32,
	emissionRate: f32,
	emissionAccumulator: f32,
	emitterShape: EmitterShape,
	shapeRadius: f32,
	coneAngle: f32,
	boxSize: Vector2,
	startLifetime: (f32, f32),
	startSpeed: (f32, f32),
	startSize: (f32, f32),
	startRotation: (f32, f32),
	startColor: (Color, Color),
	colorOverLifetime: Vec<(f32, Color)>,
	sizeOverLifetime: (f32, f32),
	angularVelocity: (f32, f32),
	gravity: Vector2,
	damping: f32,
	image: Option<Rc<Image>>,
	blendMode: BlendMode,
	maxParticleCount: usize,
	burstList: Vec<ParticleBurst>,
	isManualTick: bool,
	random: Option<SeededRandom>,
	randomSeed: Option<u32>,
}

impl ParticleSystem {
	//==============================================================================
	// 생성. (재생 중, 반복, 초당 20 개)
	//==============================================================================
	pub fn new() -> ParticleSystem {
		ParticleSystem {
			componentBase: ComponentBase::new(),
			particles: Vec::new(),
			isPlaying: true,
			isLooping: true,
			duration: 1.0,
			elapsedTime: 0.0,
			emissionRate: 20.0,
			emissionAccumulator: 0.0,
			emitterShape: EmitterShape::point,
			shapeRadius: 0.0,
			coneAngle: math::degreeToRadian(30.0),
			boxSize: Vector2::zero(),
			startLifetime: (1.0, 1.0),
			startSpeed: (100.0, 100.0),
			startSize: (16.0, 16.0),
			startRotation: (0.0, 0.0),
			startColor: (Color::white(), Color::white()),
			colorOverLifetime: Vec::new(),
			sizeOverLifetime: (1.0, 1.0),
			angularVelocity: (0.0, 0.0),
			gravity: Vector2::zero(),
			damping: 0.0,
			image: None,
			blendMode: BlendMode::normal,
			maxParticleCount: 1000,
			burstList: Vec::new(),
			isManualTick: false,
			random: None,
			randomSeed: None,
		}
	}

	//==============================================================================
	// 재생. (방출 시작)
	//==============================================================================
	pub fn play(&mut self) {
		self.isPlaying = true;
		self.elapsedTime = 0.0;
		self.emissionAccumulator = 0.0;
		for burst in self.burstList.iter_mut() {
			burst.isFired = false;
		}
	}

	//==============================================================================
	// 정지. (isClearing 이면 살아 있는 입자도 지운다)
	//==============================================================================
	pub fn stop(&mut self, isClearing: bool) {
		self.isPlaying = false;
		if isClearing {
			self.particles.clear();
			self.emissionAccumulator = 0.0;
			if let (Some(random), Some(randomSeed)) = (self.random.as_mut(), self.randomSeed) {
				random.setSeed(randomSeed);
			}
		}
	}

	//==============================================================================
	// 즉시 방출. (재생 여부와 상관없이 count 개)
	//==============================================================================
	pub fn emit(&mut self, count: usize, emitterCenter: Vector2) {
		for _ in 0..count {
			if self.particles.len() >= self.maxParticleCount {
				break;
			}
			let particle = self.createParticle(emitterCenter);
			self.particles.push(particle);
		}
	}

	//==============================================================================
	// 입자 하나 만들기.
	//==============================================================================
	fn createParticle(&mut self, emitterCenter: Vector2) -> Particle {
		let (offset, directionRadian) = match self.emitterShape {
			EmitterShape::point => {
				(Vector2::zero(), self.randomRange(0.0, math::PI * 2.0))
			}
			EmitterShape::circle => {
				let radian = self.randomRange(0.0, math::PI * 2.0);
				let distance = self.shapeRadius * self.randomRange(0.0, 1.0).sqrt();
				(Vector2::create(radian.cos() * distance, radian.sin() * distance), radian)
			}
			EmitterShape::cone => {
				let halfConeAngle = self.coneAngle * 0.5;
				let radian = -math::PI * 0.5 + self.randomRange(-halfConeAngle, halfConeAngle);
				(Vector2::zero(), radian)
			}
			EmitterShape::rectangle => {
				let halfConeAngle = self.coneAngle * 0.5;
				let radian = -math::PI * 0.5 + self.randomRange(-halfConeAngle, halfConeAngle);
				let offsetX = self.randomRange(-0.5, 0.5) * self.boxSize.x;
				let offsetY = self.randomRange(-0.5, 0.5) * self.boxSize.y;
				(Vector2::create(offsetX, offsetY), radian)
			}
		};
		let speed = self.randomBetween(self.startSpeed);
		let colorMix = self.randomRange(0.0, 1.0);
		let rotation = self.randomBetween(self.startRotation);
		let angularVelocity = self.randomBetween(self.angularVelocity);
		let startSize = self.randomBetween(self.startSize);
		let lifetime = self.randomBetween(self.startLifetime).max(0.01);
		Particle {
			position: emitterCenter.add(offset),
			velocity: Vector2::create(directionRadian.cos() * speed, directionRadian.sin() * speed),
			rotation,
			angularVelocity,
			startSize,
			startColor: Color::lerp(self.startColor.0, self.startColor.1, colorMix),
			age: 0.0,
			lifetime,
		}
	}

	//==============================================================================
	// 스폰용 난수. (씨앗이 있으면 씨앗 난수, 없으면 전역 난수, 원본 randomRange)
	//==============================================================================
	pub fn randomRange(&mut self, minimumValue: f32, maximumValue: f32) -> f32 {
		match self.random.as_mut() {
			Some(random) => {
				random.nextRange(minimumValue, maximumValue)
			}
			None => {
				math::randomRange(minimumValue, maximumValue)
			}
		}
	}

	//==============================================================================
	// (최소, 최대) 사이 스폰용 난수.
	//==============================================================================
	fn randomBetween(&mut self, range: (f32, f32)) -> f32 {
		self.randomRange(range.0, range.1)
	}

	//==============================================================================
	// 시간 진행. (방출 + 버스트 + 적분. 수동 tick 이면 밖에서 부른다. 방출점은 노드 로컬 좌표)
	//==============================================================================
	pub fn simulate(&mut self, timeDelta: f32, emitterCenter: Vector2) {
		// 방출.
		if self.isPlaying {
			let previousElapsedTime = self.elapsedTime;
			self.elapsedTime += timeDelta;
			let isEmitWindow = self.isLooping || self.elapsedTime < self.duration;
			if isEmitWindow && self.emissionRate > 0.0 {
				self.emissionAccumulator += self.emissionRate * timeDelta;
				let emitCount = self.emissionAccumulator.floor();
				self.emissionAccumulator -= emitCount;
				self.emit(emitCount as usize, emitterCenter);
			}
			self.fireBursts(previousElapsedTime, emitterCenter);
			if !isEmitWindow {
				self.isPlaying = false;
			}
		}

		// 입자 갱신.
		let gravity = self.gravity;
		let dampingFactor = (1.0 - self.damping * timeDelta).max(0.0);
		for particle in self.particles.iter_mut() {
			particle.age += timeDelta;
			particle.velocity = particle.velocity.add(gravity.multiplyScalar(timeDelta)).multiplyScalar(dampingFactor);
			particle.position = particle.position.add(particle.velocity.multiplyScalar(timeDelta));
			particle.rotation += particle.angularVelocity * timeDelta;
		}
		self.particles.retain(|particle| particle.age < particle.lifetime);
	}

	//==============================================================================
	// 버스트 발사. (지난 시간 ~ 지금 사이에 든 버스트, 반복이면 duration 주기로 다시 든다. 원본 simulate 의 버스트 부분)
	//==============================================================================
	fn fireBursts(&mut self, previousElapsedTime: f32, emitterCenter: Vector2) {
		let mut emitCountList: Vec<usize> = Vec::new();
		let isCycling = self.isLooping && self.duration > 0.0;
		for burst in self.burstList.iter_mut() {
			let mut currentTime = self.elapsedTime;
			let mut previousTime = previousElapsedTime;
			if isCycling {
				currentTime = self.elapsedTime % self.duration;
				previousTime = previousElapsedTime % self.duration;
				if currentTime < previousTime {
					burst.isFired = false;
				}
			}
			if !burst.isFired && previousTime <= burst.time && currentTime >= burst.time {
				burst.isFired = true;
				emitCountList.push(burst.count);
			}
		}
		for emitCount in emitCountList {
			self.emit(emitCount, emitterCenter);
		}
	}

	//==============================================================================
	// 수명 비율에 맞는 색. (색 곡선이 없으면 시작 색)
	//==============================================================================
	fn evaluateColor(&self, startColor: Color, lifeRatio: f32) -> Color {
		if self.colorOverLifetime.is_empty() {
			return startColor;
		}
		let firstStop = self.colorOverLifetime[0];
		if lifeRatio <= firstStop.0 {
			return multiplyColor(startColor, firstStop.1);
		}
		for stopIndex in 1..self.colorOverLifetime.len() {
			let previousStop = self.colorOverLifetime[stopIndex - 1];
			let nextStop = self.colorOverLifetime[stopIndex];
			if lifeRatio <= nextStop.0 {
				let span = (nextStop.0 - previousStop.0).max(0.0001);
				let stopRatio = (lifeRatio - previousStop.0) / span;
				let gradientColor = Color::lerp(previousStop.1, nextStop.1, stopRatio);
				return multiplyColor(startColor, gradientColor);
			}
		}
		let lastStop = self.colorOverLifetime[self.colorOverLifetime.len() - 1];
		multiplyColor(startColor, lastStop.1)
	}

	pub fn setEmissionRate(&mut self, emissionRate: f32) {
		self.emissionRate = emissionRate.max(0.0);
	}

	pub fn setEmitterShape(&mut self, emitterShape: EmitterShape) {
		self.emitterShape = emitterShape;
	}

	pub fn setShapeRadius(&mut self, shapeRadius: f32) {
		self.shapeRadius = shapeRadius.max(0.0);
	}

	pub fn setConeAngle(&mut self, coneAngleRadian: f32) {
		self.coneAngle = coneAngleRadian;
	}

	pub fn setBoxSize(&mut self, width: f32, height: f32) {
		self.boxSize = Vector2::create(width, height);
	}

	pub fn setStartLifetime(&mut self, minimumSeconds: f32, maximumSeconds: f32) {
		self.startLifetime = (minimumSeconds, maximumSeconds);
	}

	pub fn setStartSpeed(&mut self, minimumSpeed: f32, maximumSpeed: f32) {
		self.startSpeed = (minimumSpeed, maximumSpeed);
	}

	pub fn setStartSize(&mut self, minimumSize: f32, maximumSize: f32) {
		self.startSize = (minimumSize, maximumSize);
	}

	pub fn setStartRotation(&mut self, minimumRadian: f32, maximumRadian: f32) {
		self.startRotation = (minimumRadian, maximumRadian);
	}

	pub fn setStartColor(&mut self, colorA: impl Into<Color>, colorB: impl Into<Color>) {
		self.startColor = (colorA.into(), colorB.into());
	}

	//==============================================================================
	// 수명에 따른 색 곡선. (수명 비율 0 ~ 1 과 색의 쌍 목록, 시작 색에 곱한다)
	//==============================================================================
	pub fn setColorOverLifetime(&mut self, gradient: Vec<(f32, Color)>) {
		self.colorOverLifetime = gradient;
	}

	pub fn setSizeOverLifetime(&mut self, startScale: f32, endScale: f32) {
		self.sizeOverLifetime = (startScale, endScale);
	}

	pub fn setAngularVelocity(&mut self, minimumRadianPerSecond: f32, maximumRadianPerSecond: f32) {
		self.angularVelocity = (minimumRadianPerSecond, maximumRadianPerSecond);
	}

	pub fn setGravity(&mut self, gravityX: f32, gravityY: f32) {
		self.gravity = Vector2::create(gravityX, gravityY);
	}

	pub fn setDamping(&mut self, damping: f32) {
		self.damping = damping.max(0.0);
	}

	//==============================================================================
	// 입자 이미지. (None 이면 부드러운 원)
	//==============================================================================
	pub fn setImage(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	// 더하기 합성 여부. (켜면 lighter)
	//==============================================================================
	pub fn setAdditive(&mut self, isAdditive: bool) {
		self.blendMode = if isAdditive {
			BlendMode::lighter
		}
		else {
			BlendMode::normal
		};
	}

	pub fn setBlendMode(&mut self, blendMode: BlendMode) {
		self.blendMode = blendMode;
	}

	pub fn setLooping(&mut self, isLooping: bool) {
		self.isLooping = isLooping;
	}

	pub fn setDuration(&mut self, duration: f32) {
		self.duration = duration.max(0.0);
	}

	pub fn setMaxParticleCount(&mut self, maxParticleCount: usize) {
		self.maxParticleCount = maxParticleCount;
	}

	pub fn getParticleCount(&self) -> usize {
		self.particles.len()
	}

	pub fn isPlaying(&self) -> bool {
		self.isPlaying
	}

	//==============================================================================
	// 다 끝났는지. (방출이 멈췄고 남은 입자도 없음, 일회성 노드를 치울 때)
	//==============================================================================
	pub fn isFinished(&self) -> bool {
		!self.isPlaying && self.particles.is_empty()
	}

	//==============================================================================
	// 버스트 목록 설정. ((재생 시작부터 초, 개수) 목록)
	//==============================================================================
	pub fn setBurstList(&mut self, burstList: Vec<(f32, usize)>) {
		self.burstList = burstList.into_iter().map(|(time, count)| ParticleBurst { time, count, isFired: false }).collect();
	}

	//==============================================================================
	// 수동 tick 설정. (켜면 노드 갱신에서 진행하지 않고 simulate 로만 진행한다. 타임라인처럼 밖에서 시간을 다룰 때)
	//==============================================================================
	pub fn setManualTick(&mut self, isManualTick: bool) {
		self.isManualTick = isManualTick;
	}

	pub fn isManualTick(&self) -> bool {
		self.isManualTick
	}

	//==============================================================================
	// 난수 씨앗 설정. (설정하면 스폰 난수가 씨앗 난수로 바뀌고 stop(true) 마다 씨앗으로 되돌아간다. 되감기 재현용)
	//==============================================================================
	pub fn setRandomSeed(&mut self, randomSeed: u32) {
		self.randomSeed = Some(randomSeed);
		self.random = Some(SeededRandom::new(randomSeed));
	}

	pub fn getRandomSeed(&self) -> Option<u32> {
		self.randomSeed
	}

	//==============================================================================
	// 서술(JSON) 적용. (원본 applyDescription. 준 항목만 반영한다. 색은 [r, g, b, a] 배열)
	// - 옮기지 않은 항목(attractor, wobble, renderShape, streakScale, worldSpace)은 읽지 않는다.
	// - coneAngle 은 원본처럼 가운데에서 한쪽으로 벌어지는 각이라, 전체 폭을 받는 setConeAngle 에는 두 배로 넣는다.
	//==============================================================================
	pub fn applyDescription(&mut self, description: &serde_json::Value) {
		let readNumber = |name: &str| description.get(name).and_then(|value| value.as_f64()).map(|value| value as f32);
		let readPair = |name: &str| -> Option<(f32, f32)> {
			let array = description.get(name)?.as_array()?;
			let first = array.first()?.as_f64()? as f32;
			let second = array.get(1).and_then(|value| value.as_f64()).map(|value| value as f32).unwrap_or(first);
			Some((first, second))
		};
		let shapeName = description.get("shape").and_then(|value| value.as_str());
		if let Some(isLooping) = description.get("looping").and_then(|value| value.as_bool()) {
			self.setLooping(isLooping);
		}
		if let Some(duration) = readNumber("duration") {
			self.setDuration(duration);
		}
		if let Some(maxParticleCount) = readNumber("maxParticleCount") {
			self.setMaxParticleCount(maxParticleCount.max(0.0) as usize);
		}
		if let Some(emissionRate) = readNumber("emissionRate") {
			self.setEmissionRate(emissionRate);
		}
		if let Some(burstArray) = description.get("bursts").and_then(|value| value.as_array()) {
			let burstList = burstArray.iter().map(|burst| {
				let time = burst.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				let count = burst.get("count").and_then(|value| value.as_f64()).unwrap_or(0.0).max(0.0) as usize;
				(time, count)
			}).collect();
			self.setBurstList(burstList);
		}
		if let Some(shapeName) = shapeName {
			if let Some((emitterShape, _)) = EMITTER_SHAPE_NAME_TABLE.iter().find(|(_, name)| *name == shapeName) {
				self.setEmitterShape(*emitterShape);
			}
		}
		if let Some(shapeRadius) = readNumber("shapeRadius") {
			self.setShapeRadius(shapeRadius);
		}
		if let Some(coneAngle) = readNumber("coneAngle") {
			self.setConeAngle(coneAngle * 2.0);
		}
		if let Some((width, height)) = readPair("boxSize") {
			self.setBoxSize(width, height);
		}
		if shapeName == Some("edge") {
			let edgeWidth = readNumber("edgeWidth").unwrap_or(0.0);
			self.setEmitterShape(EmitterShape::rectangle);
			self.setBoxSize(edgeWidth, 0.0);
		}
		if let Some((minimum, maximum)) = readPair("lifetime") {
			self.setStartLifetime(minimum, maximum);
		}
		if let Some((minimum, maximum)) = readPair("speed") {
			self.setStartSpeed(minimum, maximum);
		}
		if let Some((minimum, maximum)) = readPair("size") {
			self.setStartSize(minimum, maximum);
		}
		if let Some((minimum, maximum)) = readPair("rotation") {
			self.setStartRotation(minimum, maximum);
		}
		if let Some((minimum, maximum)) = readPair("angularVelocity") {
			self.setAngularVelocity(minimum, maximum);
		}
		if let Some(colorA) = description.get("startColorA").and_then(readDescriptionColor) {
			let colorB = description.get("startColorB").and_then(readDescriptionColor).unwrap_or(colorA);
			self.setStartColor(colorA, colorB);
		}
		if let Some(keyArray) = description.get("colorOverLifetime").and_then(|value| value.as_array()) {
			let gradient = keyArray.iter().filter_map(|key| {
				let time = key.get("time").and_then(|value| value.as_f64()).unwrap_or(0.0) as f32;
				let color = key.get("color").and_then(readDescriptionColor)?;
				Some((time, color))
			}).collect();
			self.setColorOverLifetime(gradient);
		}
		if let Some((startScale, endScale)) = readPair("sizeOverLifetime") {
			self.setSizeOverLifetime(startScale, endScale);
		}
		if let Some((gravityX, gravityY)) = readPair("gravity") {
			self.setGravity(gravityX, gravityY);
		}
		if let Some(damping) = readNumber("damping") {
			self.setDamping(damping);
		}
		if let Some(blendMode) = description.get("blendMode").and_then(|value| value.as_str()).and_then(BlendMode::fromName) {
			self.setBlendMode(blendMode);
		}
	}
}

impl Default for ParticleSystem {
	fn default() -> ParticleSystem {
		ParticleSystem::new()
	}
}

impl Component for ParticleSystem {
	fn getComponentBase(&self) -> &ComponentBase {
		&self.componentBase
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		&mut self.componentBase
	}

	fn getComponentType(&self) -> &'static str {
		"ParticleSystem"
	}

	fn tick(&mut self, context: &mut ComponentContext, timeDelta: f32) {
		if self.isManualTick {
			return;
		}
		let emitterCenter = context.getTransform().getContentSize().multiplyScalar(0.5);
		self.simulate(timeDelta, emitterCenter);
	}

	fn draw(&mut self, _transform: &NodeTransform, graphic: &mut Graphic) {
		if self.particles.is_empty() {
			return;
		}
		let texture = match &self.image {
			Some(image) => {
				match graphic.getImageTexture(image) {
					Some(texture) => {
						texture
					}
					None => {
						return;
					}
				}
			}
			None => {
				graphic.getSoftDiscTexture()
			}
		};

		// 입자마다 쿼드 한 장. (회전한 네 모서리 + 정점 색)
		let particleCount = self.particles.len();
		let mut vertexValues: Vec<f32> = Vec::with_capacity(particleCount * 6 * PARTICLE_FLOATS_PER_VERTEX);
		for particle in self.particles.iter() {
			let lifeRatio = math::clamp01(particle.age / particle.lifetime);
			let size = particle.startSize * math::lerp(self.sizeOverLifetime.0, self.sizeOverLifetime.1, lifeRatio);
			let color = self.evaluateColor(particle.startColor, lifeRatio);
			let halfSize = size * 0.5;
			let cosine = particle.rotation.cos() * halfSize;
			let sine = particle.rotation.sin() * halfSize;
			let centerX = particle.position.x;
			let centerY = particle.position.y;
			let corners: [(f32, f32, f32, f32); 4] = [
				(centerX - cosine + sine, centerY - sine - cosine, 0.0, 0.0),
				(centerX + cosine + sine, centerY + sine - cosine, 1.0, 0.0),
				(centerX - cosine - sine, centerY - sine + cosine, 0.0, 1.0),
				(centerX + cosine - sine, centerY + sine + cosine, 1.0, 1.0),
			];
			for cornerIndex in [0usize, 1, 2, 2, 1, 3] {
				let (cornerX, cornerY, cornerU, cornerV) = corners[cornerIndex];
				vertexValues.extend_from_slice(&[cornerX, cornerY, cornerU, cornerV, color.red, color.green, color.blue, color.alpha]);
			}
		}

		let particleVertexData = graphic.getParticleVertexData(vertexValues.len());
		let writableFloatCount = particleVertexData.len();
		particleVertexData.copy_from_slice(&vertexValues[..writableFloatCount]);
		let vertexCount = writableFloatCount / PARTICLE_FLOATS_PER_VERTEX;

		let previousBlendMode = graphic.getBlendMode();
		graphic.setBlendMode(self.blendMode);
		graphic.drawColoredQuads(vertexCount, texture);
		graphic.setBlendMode(previousBlendMode);
	}
}

//==============================================================================
// 서술의 색 배열 읽기. ([r, g, b] 또는 [r, g, b, a], 원본 toColor)
//==============================================================================
fn readDescriptionColor(value: &serde_json::Value) -> Option<Color> {
	let channels = value.as_array()?;
	let readChannel = |channelIndex: usize, defaultValue: f32| channels.get(channelIndex).and_then(|channel| channel.as_f64()).map(|channel| channel as f32).unwrap_or(defaultValue);
	Some(Color::create(readChannel(0, 0.0), readChannel(1, 0.0), readChannel(2, 0.0), readChannel(3, 1.0)))
}

//==============================================================================
// 색 성분별 곱.
//==============================================================================
fn multiplyColor(colorA: Color, colorB: Color) -> Color {
	Color::create(colorA.red * colorB.red, colorA.green * colorB.green, colorA.blue * colorB.blue, colorA.alpha * colorB.alpha)
}
