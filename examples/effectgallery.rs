//==============================================================================
// 효과 갤러리 예제. (ShaderSprite, ScreenEffect, TrailRenderer 눈으로 확인)
// - 왼쪽: ShaderSpriteEffect 50 가지를 칸마다 하나씩 건다. (none 은 원본)
//   그림은 코드로 만든 두 칸짜리 시트이고 모든 칸이 왼쪽 칸(image_rect)만 쓴다. 오른쪽 칸은 (경계 두 줄을 빼고) 새빨간 사각형이라 textureRect 자르기가 틀리면 빨강이 번져 보인다.
// - 오른쪽: 둥근 모서리 무대에만 ScreenEffect 를 건다. (set_region) 30 프레임마다 효과 묶음이 바뀌고, 120 프레임 묶음은 화면 전체(clear_region)에 건다.
//   3 프레임째에는 모든 효과를 한꺼번에 켜서 패스 셰이더가 다 컴파일되는지 본다. (실패하면 stderr 에 ScreenEffect: pass ... failed)
// - 아래 왼쪽: TrailRenderer 둘. (보통 합성, 더하기 합성) 무대 안에도 더하기 합성 트레일이 하나 돈다.
// - 아래 오른쪽: ParticleSystem 칸 여덟. 렌더 모양 넷(circle 분수, rect 색종이, streak 불티, image 별)과 끌개(소용돌이), 흔들림(물방울), edge(눈), 월드 공간(도는 방출점이 남기는 자국).
//   저장한 화면이 실행마다 같게 수동 tick 으로 프레임마다 1/60 초씩, 씨앗을 고정해 흘리고, 처음 2 초를 미리 흘려 둔다.
// - 실행: cargo run --release --example effectgallery
//   화면 저장: VANILLA_CAPTURE_PATH=shots/effect_{frame}.png VANILLA_CAPTURE_FRAMES=60,120 VANILLA_CAPTURE_EXIT=1
//==============================================================================

use std::rc::Rc;
use vanilla::core::graphic::TextAlign;
use vanilla::{math, Color, Component, ComponentBase, ComponentContext, Engine, EngineConfiguration, Graphic, Image, Mask, NodeId, Paint, ParticleEmitterShape, ParticleRenderShape, ParticleSystem, Pivot, Rect, Scene, SceneBase, ScreenEffect, ScreenEffectType, ShaderSprite, ShaderSpriteEffect, Sprite, Text, TrailRenderer, Vector2, ViewScaleMode};

//==============================================================================
// 전역 상수 목록.
//==============================================================================
const VIEW_WIDTH: f32 = 1280.0;
const VIEW_HEIGHT: f32 = 720.0;
const BACKGROUND_COLOR: &str = "#1b1f27";
const CELL_COLOR: &str = "#3a4150";
const LABEL_COLOR: &str = "#c9d1e0";
const GRID_COLUMN_COUNT: usize = 10;
const GRID_LEFT: f32 = 15.0;
const GRID_TOP: f32 = 44.0;
const CELL_WIDTH: f32 = 84.0;
const CELL_HEIGHT: f32 = 101.0;
const SPRITE_SIZE: f32 = 64.0;
const SHEET_FRAME_SIZE: u32 = 80;
const SHEET_GUTTER_SIZE: u32 = 2;
const STAGE_X: f32 = 870.0;
const STAGE_Y: f32 = 44.0;
const STAGE_WIDTH: f32 = 395.0;
const STAGE_HEIGHT: f32 = 505.0;
const STAGE_CORNER_RADIUS: f32 = 16.0;
const EFFECT_SET_FRAME_COUNT: u64 = 30;
const EFFECT_SET_COUNT: usize = 8;
const COMPILE_CHECK_FRAME: u64 = 3;
// 움직이는 노드가 프레임마다 가는 시간. (창이 뜨기 전 첫 프레임들은 수직 동기 없이 빨리 돌아서, 실제 시간으로 움직이면 저장한 프레임 모양이 실행마다 다르다)
const MOVER_FRAME_STEP: f32 = 1.0 / 60.0;
// 아래 트레일이 오가는 오른쪽 끝. (그 오른쪽은 파티클 칸)
const TRAIL_END_X: f32 = 410.0;
// 파티클 칸 줄.
const PARTICLE_ROW_LEFT: f32 = 455.0;
const PARTICLE_ROW_TOP: f32 = 588.0;
const PARTICLE_CELL_STEP: f32 = 101.25;
const PARTICLE_CELL_WIDTH: f32 = 97.0;
const PARTICLE_CELL_HEIGHT: f32 = 124.0;
const PARTICLE_STAR_SIZE: u32 = 32;
// 파티클을 미리 흘려 두는 시간. (초)
const PARTICLE_WARM_UP_SECONDS: f32 = 2.0;

//==============================================================================
// 원을 따라 도는 노드. (무대 안 트레일)
//==============================================================================
struct OrbitMover {
	component_base: ComponentBase,
	center: Vector2,
	radius: f32,
	angular_speed: f32,
	angle: f32,
}

#[vanilla::reflect]
impl OrbitMover {}

#[vanilla::reflect]
impl Component for OrbitMover {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"OrbitMover"
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		self.angle += self.angular_speed * MOVER_FRAME_STEP;
		let radius_scale = 1.0 + 0.25 * (self.angle * 2.0).sin();
		let position_x = self.center.x + self.angle.cos() * self.radius * radius_scale;
		let position_y = self.center.y + self.angle.sin() * self.radius * 0.8;
		context.get_transform_mut().set_local_position(Vector2::create(position_x, position_y));
	}
}

//==============================================================================
// 물결치며 좌우로 오가는 노드. (아래 트레일)
//==============================================================================
struct WaveMover {
	component_base: ComponentBase,
	elapsed_time: f32,
	start_x: f32,
	end_x: f32,
	horizontal_frequency: f32,
	base_y: f32,
	amplitude: f32,
	frequency: f32,
}

#[vanilla::reflect]
impl WaveMover {}

#[vanilla::reflect]
impl Component for WaveMover {
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	fn get_component_type(&self) -> &'static str {
		"WaveMover"
	}

	fn tick(&mut self, context: &mut ComponentContext, _time_delta: f32) {
		self.elapsed_time += MOVER_FRAME_STEP;
		let travel_ratio = 0.5 - 0.5 * (self.elapsed_time * self.horizontal_frequency).cos();
		let position_x = self.start_x + (self.end_x - self.start_x) * travel_ratio;
		let position_y = self.base_y + (self.elapsed_time * self.frequency).sin() * self.amplitude;
		context.get_transform_mut().set_local_position(Vector2::create(position_x, position_y));
	}
}

//==============================================================================
// 두 칸짜리 그림 시트 만들기. (왼쪽: 안테나 달린 둥근 캐릭터, 오른쪽: 새빨간 사각형)
//==============================================================================
fn create_character_sheet() -> Rc<Image> {
	let frame_size = SHEET_FRAME_SIZE;
	let sheet_width = frame_size * 2;
	let mut pixels = vec![0u8; (sheet_width * frame_size * 4) as usize];
	let center = frame_size as f32 * 0.5;
	for pixel_y in 0..frame_size {
		for pixel_x in 0..sheet_width {
			let pixel_index = ((pixel_y * sheet_width + pixel_x) * 4) as usize;
			if pixel_x >= frame_size {
				// 맞닿은 두 줄은 비워 둔다. (쌍선형 필터가 칸 경계에서 옆 칸을 섞는 것은 효과와 상관없는 아틀라스 번짐이라 빼고 본다)
				if pixel_x >= frame_size + SHEET_GUTTER_SIZE {
					pixels[pixel_index] = 255;
					pixels[pixel_index + 3] = 255;
				}
				continue;
			}
			let sample_x = pixel_x as f32 + 0.5;
			let sample_y = pixel_y as f32 + 0.5;

			// 몸통. (가장자리 부드럽게, 위왼쪽 하늘색 → 아래오른쪽 보라)
			let body_distance = ((sample_x - center).powi(2) + (sample_y - center - 4.0).powi(2)).sqrt();
			let body_coverage = math::clamp01(28.0 - body_distance + 0.5);
			let gradient = (sample_x + sample_y) / (frame_size as f32 * 2.0);
			let mut color = Color::lerp(Color::create(0.35, 0.88, 1.0, 1.0), Color::create(0.78, 0.32, 0.95, 1.0), gradient);
			if body_distance > 23.0 {
				color = Color::create(color.red * 0.6, color.green * 0.6, color.blue * 0.6, 1.0);
			}

			// 하이라이트, 눈.
			let highlight_distance = ((sample_x - 30.0).powi(2) + (sample_y - 30.0).powi(2)).sqrt();
			if highlight_distance < 6.0 {
				color = Color::lerp(color, Color::white(), 0.8);
			}
			let left_eye_distance = ((sample_x - 32.0).powi(2) + (sample_y - 46.0).powi(2)).sqrt();
			let right_eye_distance = ((sample_x - 48.0).powi(2) + (sample_y - 46.0).powi(2)).sqrt();
			if left_eye_distance < 4.0 || right_eye_distance < 4.0 {
				color = Color::create(0.05, 0.05, 0.1, 1.0);
			}

			// 안테나. (줄기 + 노란 공)
			let stem_coverage = if (sample_x - center).abs() < 1.5 && sample_y > 8.0 && sample_y < 18.0 {
				1.0
			}
			else {
				0.0
			};
			let ball_distance = ((sample_x - center).powi(2) + (sample_y - 9.0).powi(2)).sqrt();
			let ball_coverage = math::clamp01(5.0 - ball_distance + 0.5);
			let mut coverage = body_coverage;
			if stem_coverage > 0.0 && body_coverage < 1.0 {
				color = Color::create(0.9, 0.9, 0.95, 1.0);
				coverage = 1.0;
			}
			if ball_coverage > 0.0 && body_coverage <= 0.0 {
				color = Color::create(1.0, 0.85, 0.2, 1.0);
				coverage = coverage.max(ball_coverage);
			}
			if coverage <= 0.0 {
				continue;
			}
			pixels[pixel_index] = (math::clamp01(color.red) * 255.0) as u8;
			pixels[pixel_index + 1] = (math::clamp01(color.green) * 255.0) as u8;
			pixels[pixel_index + 2] = (math::clamp01(color.blue) * 255.0) as u8;
			pixels[pixel_index + 3] = (coverage * 255.0) as u8;
		}
	}
	Image::create_from_pixels(sheet_width, frame_size, pixels)
}

//==============================================================================
// 별 그림 만들기. (다섯 뿔, 가장자리 부드럽게. 파티클 image 모양용)
//==============================================================================
fn create_star_image() -> Rc<Image> {
	let image_size = PARTICLE_STAR_SIZE;
	let mut pixels = vec![0u8; (image_size * image_size * 4) as usize];
	let center = image_size as f32 * 0.5;
	for pixel_y in 0..image_size {
		for pixel_x in 0..image_size {
			let delta_x = pixel_x as f32 + 0.5 - center;
			let delta_y = pixel_y as f32 + 0.5 - center;
			let distance = (delta_x * delta_x + delta_y * delta_y).sqrt();
			let angle = delta_y.atan2(delta_x) + math::PI * 0.5;
			let point_ratio = (0.5 + 0.5 * (angle * 5.0).cos()).powi(3);
			let edge_distance = center * (0.42 + 0.56 * point_ratio);
			let coverage = math::clamp01(edge_distance - distance + 0.5);
			if coverage <= 0.0 {
				continue;
			}
			let pixel_index = ((pixel_y * image_size + pixel_x) * 4) as usize;
			let shine = math::clamp01(1.0 - distance / center);
			pixels[pixel_index] = 255;
			pixels[pixel_index + 1] = (200.0 + 55.0 * shine) as u8;
			pixels[pixel_index + 2] = (60.0 + 160.0 * shine) as u8;
			pixels[pixel_index + 3] = (coverage * 255.0) as u8;
		}
	}
	Image::create_from_pixels(image_size, image_size, pixels)
}

//==============================================================================
// 칸 효과의 진행도. (사라짐 / 드러남은 절반, 광택은 띠가 가운데, 섬광은 조금 덜)
//==============================================================================
fn get_gallery_effect_progress(effect: ShaderSpriteEffect) -> f32 {
	match effect {
		ShaderSpriteEffect::Dissolve | ShaderSpriteEffect::Burn | ShaderSpriteEffect::NoiseFade | ShaderSpriteEffect::PixelDissolve | ShaderSpriteEffect::Wipe | ShaderSpriteEffect::Iris | ShaderSpriteEffect::DiamondWipe | ShaderSpriteEffect::ClockWipe | ShaderSpriteEffect::Blinds | ShaderSpriteEffect::CheckerWipe => {
			0.5
		}
		ShaderSpriteEffect::Shine => {
			0.45
		}
		ShaderSpriteEffect::Flash => {
			0.7
		}
		_ => {
			1.0
		}
	}
}

//==============================================================================
// 효과 갤러리 씬.
//==============================================================================
struct EffectGalleryScene {
	scene_base: SceneBase,
	screen_effect: ScreenEffect,
	applied_effect_set_index: Option<usize>,
	is_full_screen_effect: bool,
	effect_set_text_id: Option<NodeId>,
	particle_node_ids: Vec<NodeId>,
}

impl EffectGalleryScene {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new() -> EffectGalleryScene {
		EffectGalleryScene {
			scene_base: SceneBase::new(),
			screen_effect: ScreenEffect::new(),
			applied_effect_set_index: None,
			is_full_screen_effect: false,
			effect_set_text_id: None,
			particle_node_ids: Vec::new(),
		}
	}

	//==============================================================================
	// 글 노드 하나 붙이기.
	//==============================================================================
	fn add_label(&mut self, parent_id: NodeId, position: Vector2, size: Vector2, text: &str, font_size: f32, text_align: TextAlign) -> NodeId {
		let node_tree = self.scene_base.get_node_tree_mut();
		let label_id = node_tree.create_child(parent_id, "label");
		let label_node = &mut node_tree[label_id];
		if text_align == TextAlign::Left {
			label_node.set_pivot(Pivot::MIDDLE_LEFT);
		}
		label_node.set_local_position(position);
		label_node.set_content_size(size);
		let label = label_node.add_component(Text::create_with_text(text, font_size));
		label.set_text_color(LABEL_COLOR);
		label.set_text_align(text_align);
		label_id
	}

	//==============================================================================
	// ShaderSprite 칸 50 개.
	//==============================================================================
	fn create_shader_sprite_grid(&mut self, root_id: NodeId, sheet_image: &Rc<Image>) {
		let frame_rect = Rect::create(0.0, 0.0, SHEET_FRAME_SIZE as f32, SHEET_FRAME_SIZE as f32);
		for (effect_index, effect) in ShaderSpriteEffect::get_effect_list().iter().enumerate() {
			let column_index = effect_index % GRID_COLUMN_COUNT;
			let row_index = effect_index / GRID_COLUMN_COUNT;
			let cell_left = GRID_LEFT + column_index as f32 * CELL_WIDTH;
			let cell_top = GRID_TOP + row_index as f32 * CELL_HEIGHT;
			let cell_center_x = cell_left + CELL_WIDTH * 0.5;

			let node_tree = self.scene_base.get_node_tree_mut();
			let cell_id = node_tree.create_child(root_id, "cell");
			let cell_node = &mut node_tree[cell_id];
			cell_node.set_local_position(Vector2::create(cell_center_x, cell_top + CELL_HEIGHT * 0.5));
			cell_node.set_content_size(Vector2::create(CELL_WIDTH - 6.0, CELL_HEIGHT - 6.0));
			let cell_paint = cell_node.add_component(Paint::create_with_color(CELL_COLOR));
			cell_paint.set_round_size(8.0);

			let sprite_id = node_tree.create_child(root_id, effect.get_name());
			let sprite_node = &mut node_tree[sprite_id];
			sprite_node.set_local_position(Vector2::create(cell_center_x, cell_top + 42.0));
			sprite_node.set_content_size(Vector2::create(SPRITE_SIZE, SPRITE_SIZE));
			let shader_sprite = sprite_node.add_component(ShaderSprite::create_with_image(sheet_image.clone()));
			shader_sprite.set_image_rect(frame_rect);
			shader_sprite.set_effect(*effect);
			shader_sprite.set_effect_progress(get_gallery_effect_progress(*effect));
			shader_sprite.set_effect_seed(effect_index as f32 * 7.31);

			self.add_label(root_id, Vector2::create(cell_center_x, cell_top + 86.0), Vector2::create(CELL_WIDTH, 14.0), effect.get_name(), 11.0, TextAlign::Center);
		}
	}

	//==============================================================================
	// ScreenEffect 무대. (둥근 판, 해, 땅, 캐릭터 셋, 도는 트레일)
	//==============================================================================
	fn create_stage(&mut self, root_id: NodeId, sheet_image: &Rc<Image>) {
		let frame_rect = Rect::create(0.0, 0.0, SHEET_FRAME_SIZE as f32, SHEET_FRAME_SIZE as f32);
		let node_tree = self.scene_base.get_node_tree_mut();
		let stage_id = node_tree.create_child(root_id, "stage");
		let stage_node = &mut node_tree[stage_id];
		stage_node.set_pivot(Pivot::TOP_LEFT);
		stage_node.set_local_position(Vector2::create(STAGE_X, STAGE_Y));
		stage_node.set_content_size(Vector2::create(STAGE_WIDTH, STAGE_HEIGHT));
		let stage_paint = stage_node.add_component(Paint::create_with_color("#16283c"));
		stage_paint.set_round_size(STAGE_CORNER_RADIUS);

		let sun_id = node_tree.create_child(stage_id, "sun");
		let sun_node = &mut node_tree[sun_id];
		sun_node.set_local_position(Vector2::create(290.0, 110.0));
		sun_node.set_content_size(Vector2::create(90.0, 90.0));
		let sun_paint = sun_node.add_component(Paint::create_with_color("#fff2b0"));
		sun_paint.set_round_size(45.0);

		let ground_id = node_tree.create_child(stage_id, "ground");
		let ground_node = &mut node_tree[ground_id];
		ground_node.set_local_position(Vector2::create(STAGE_WIDTH * 0.5, STAGE_HEIGHT - 45.0));
		ground_node.set_content_size(Vector2::create(STAGE_WIDTH - 20.0, 70.0));
		let ground_paint = ground_node.add_component(Paint::create_with_color("#2f5d3a"));
		ground_paint.set_round_size(10.0);

		let character_list: [(f32, f32, f32, ShaderSpriteEffect); 3] = [(90.0, 400.0, 96.0, ShaderSpriteEffect::None), (200.0, 330.0, 72.0, ShaderSpriteEffect::Glow), (310.0, 400.0, 96.0, ShaderSpriteEffect::Hologram)];
		for (position_x, position_y, size, effect) in character_list {
			let character_id = node_tree.create_child(stage_id, "character");
			let character_node = &mut node_tree[character_id];
			character_node.set_local_position(Vector2::create(position_x, position_y));
			character_node.set_content_size(Vector2::create(size, size));
			if effect == ShaderSpriteEffect::None {
				let sprite = character_node.add_component(Sprite::create_with_image(sheet_image.clone()));
				sprite.set_image_rect(frame_rect);
			}
			else {
				let shader_sprite = character_node.add_component(ShaderSprite::create_with_image(sheet_image.clone()));
				shader_sprite.set_image_rect(frame_rect);
				shader_sprite.set_effect(effect);
				shader_sprite.set_effect_progress(1.0);
			}
		}

		// 도는 트레일. (머리 점은 자식 노드라 트레일 원점과 겹친다. 움직이는 컴포넌트를 먼저 붙여 트레일이 옮긴 뒤의 위치를 기록하게 한다)
		let orbit_center = Vector2::create(STAGE_WIDTH * 0.5, 220.0);
		let orbit_radius = 130.0;
		let orbit_id = node_tree.create_child(stage_id, "orbit");
		let orbit_node = &mut node_tree[orbit_id];
		orbit_node.set_local_position(Vector2::create(orbit_center.x + orbit_radius, orbit_center.y));
		orbit_node.add_component(OrbitMover {
			component_base: ComponentBase::new(),
			center: orbit_center,
			radius: orbit_radius,
			angular_speed: 3.2,
			angle: 0.0,
		});
		let trail = orbit_node.add_component(TrailRenderer::new());
		trail.set_time(0.6);
		trail.set_width(18.0, 0.0);
		trail.set_additive(true);
		trail.set_colors(Color::create(0.4, 0.85, 1.0, 1.0), Color::create(0.2, 0.3, 1.0, 0.0));
		let head_id = node_tree.create_child(orbit_id, "head");
		let head_node = &mut node_tree[head_id];
		head_node.set_content_size(Vector2::create(14.0, 14.0));
		let head_paint = head_node.add_component(Paint::create_with_color(Color::white()));
		head_paint.set_round_size(7.0);

		self.add_label(stage_id, Vector2::create(STAGE_WIDTH * 0.5, 24.0), Vector2::create(STAGE_WIDTH, 20.0), "ScreenEffect 무대", 16.0, TextAlign::Center);
	}

	//==============================================================================
	// 아래 트레일 둘과 설명 글.
	//==============================================================================
	fn create_trail_showcase(&mut self, root_id: NodeId) {
		let trail_setting_list: [(f32, f32, f32, f32, bool, Color, Color); 2] = [
			(610.0, 32.0, 5.0, 16.0, false, Color::create(0.4, 0.8, 1.0, 0.9), Color::create(0.4, 0.8, 1.0, 0.0)),
			(670.0, 28.0, 4.0, 26.0, true, Color::create(1.0, 0.92, 0.6, 1.0), Color::create(1.0, 0.55, 0.2, 0.0)),
		];
		for (base_y, amplitude, frequency, width, is_additive, start_color, end_color) in trail_setting_list {
			let node_tree = self.scene_base.get_node_tree_mut();
			let mover_id = node_tree.create_child(root_id, "trailMover");
			let mover_node = &mut node_tree[mover_id];
			mover_node.set_local_position(Vector2::create(GRID_LEFT + 40.0, base_y));
			mover_node.add_component(WaveMover {
				component_base: ComponentBase::new(),
				elapsed_time: 0.0,
				start_x: GRID_LEFT + 40.0,
				end_x: TRAIL_END_X,
				horizontal_frequency: 1.3,
				base_y,
				amplitude,
				frequency,
			});
			let trail = mover_node.add_component(TrailRenderer::new());
			trail.set_time(0.7);
			trail.set_width(width, 0.0);
			trail.set_additive(is_additive);
			trail.set_colors(start_color, end_color);
		}
		self.add_label(root_id, Vector2::create(GRID_LEFT + 4.0, 575.0), Vector2::create(400.0, 16.0), "TrailRenderer: 위 보통 합성, 아래 더하기 합성", 13.0, TextAlign::Left);
		let effect_set_text_id = self.add_label(root_id, Vector2::create(STAGE_X + 4.0, 575.0), Vector2::create(STAGE_WIDTH, 16.0), "", 13.0, TextAlign::Left);
		self.effect_set_text_id = Some(effect_set_text_id);
	}

	//==============================================================================
	// 파티클 칸 하나. (둥근 바탕과 자르기, 아래 이름. 파티클 노드를 칸 왼쪽 위 기준 position 에 만들어 돌려준다)
	//==============================================================================
	fn add_particle_cell(&mut self, root_id: NodeId, cell_index: usize, title: &str, position: Vector2) -> NodeId {
		let cell_left = PARTICLE_ROW_LEFT + cell_index as f32 * PARTICLE_CELL_STEP;
		let node_tree = self.scene_base.get_node_tree_mut();
		let cell_id = node_tree.create_child(root_id, "particleCell");
		let cell_node = &mut node_tree[cell_id];
		cell_node.set_pivot(Pivot::TOP_LEFT);
		cell_node.set_local_position(Vector2::create(cell_left, PARTICLE_ROW_TOP));
		cell_node.set_content_size(Vector2::create(PARTICLE_CELL_WIDTH, PARTICLE_CELL_HEIGHT));
		let cell_paint = cell_node.add_component(Paint::create_with_color(CELL_COLOR));
		cell_paint.set_round_size(8.0);
		cell_node.add_component(Mask::new()).set_round_size(8.0);

		let particle_id = node_tree.create_child(cell_id, "particle");
		node_tree[particle_id].set_local_position(position);
		self.add_label(cell_id, Vector2::create(PARTICLE_CELL_WIDTH * 0.5, PARTICLE_CELL_HEIGHT - 10.0), Vector2::create(PARTICLE_CELL_WIDTH, 14.0), title, 12.0, TextAlign::Center);
		particle_id
	}

	//==============================================================================
	// 파티클 붙이기. (수동 tick, 칸마다 다른 씨앗, 처음 몇 초를 미리 흘린다)
	//==============================================================================
	fn attach_particle_system(&mut self, particle_id: NodeId, particle_system: ParticleSystem) {
		let mut particle_system = particle_system;
		particle_system.set_manual_tick(true);
		particle_system.set_random_seed(100 + self.particle_node_ids.len() as u32);
		let mut warm_up_seconds = 0.0;
		while warm_up_seconds < PARTICLE_WARM_UP_SECONDS {
			particle_system.simulate(MOVER_FRAME_STEP);
			warm_up_seconds += MOVER_FRAME_STEP;
		}
		self.scene_base.get_node_tree_mut()[particle_id].add_component(particle_system);
		self.particle_node_ids.push(particle_id);
	}

	//==============================================================================
	// 파티클 칸 여덟. (렌더 모양 넷, 끌개, 흔들림, edge, 월드 공간)
	//==============================================================================
	fn create_particle_showcase(&mut self, root_id: NodeId) {
		let cell_center_x = PARTICLE_CELL_WIDTH * 0.5;
		let fade_out = vec![(0.0, Color::white()), (0.7, Color::white()), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))];

		// circle: 위로 뿜었다 떨어지는 분수.
		let mut fountain = ParticleSystem::new();
		fountain.set_emitter_shape(ParticleEmitterShape::Cone);
		fountain.set_cone_angle(math::degree_to_radian(15.0));
		fountain.set_emission_rate(70.0);
		fountain.set_start_speed(150.0, 190.0);
		fountain.set_gravity(0.0, 260.0);
		fountain.set_start_lifetime(1.0, 1.3);
		fountain.set_start_size(4.0, 7.0);
		fountain.set_start_color(Color::create(1.0, 0.85, 0.4, 1.0), Color::create(1.0, 0.5, 0.2, 1.0));
		fountain.set_color_over_lifetime(fade_out.clone());
		let fountain_id = self.add_particle_cell(root_id, 0, "circle", Vector2::create(cell_center_x, 100.0));
		self.attach_particle_system(fountain_id, fountain);

		// rect: 돌며 떨어지는 색종이.
		let mut confetti = ParticleSystem::new();
		confetti.set_emitter_shape(ParticleEmitterShape::Edge);
		confetti.set_edge_width(90.0);
		confetti.set_emission_rate(24.0);
		confetti.set_start_speed(15.0, 35.0);
		confetti.set_gravity(0.0, 30.0);
		confetti.set_start_lifetime(3.0, 3.5);
		confetti.set_start_size(6.0, 9.0);
		confetti.set_start_rotation(0.0, math::PI * 2.0);
		confetti.set_angular_velocity(-6.0, 6.0);
		confetti.set_start_color(Color::create(1.0, 0.35, 0.45, 1.0), Color::create(0.35, 0.75, 1.0, 1.0));
		confetti.set_wobble(6.0, 3.0);
		confetti.set_render_shape(ParticleRenderShape::Rect);
		let confetti_id = self.add_particle_cell(root_id, 1, "rect", Vector2::create(cell_center_x, -6.0));
		self.attach_particle_system(confetti_id, confetti);

		// streak: 사방으로 튀는 불티. (더하기 합성)
		let mut sparks = ParticleSystem::new();
		sparks.set_emission_rate(50.0);
		sparks.set_start_speed(90.0, 170.0);
		sparks.set_damping(1.8);
		sparks.set_gravity(0.0, 60.0);
		sparks.set_start_lifetime(0.5, 0.9);
		sparks.set_start_size(2.0, 3.0);
		sparks.set_start_color(Color::create(1.0, 0.9, 0.5, 1.0), Color::create(1.0, 0.6, 0.2, 1.0));
		sparks.set_color_over_lifetime(fade_out.clone());
		sparks.set_render_shape(ParticleRenderShape::Streak);
		sparks.set_streak_scale(0.08);
		sparks.set_additive(true);
		let sparks_id = self.add_particle_cell(root_id, 2, "streak", Vector2::create(cell_center_x, 56.0));
		self.attach_particle_system(sparks_id, sparks);

		// image: 돌며 퍼지는 별 그림.
		let mut stars = ParticleSystem::new();
		stars.set_emitter_shape(ParticleEmitterShape::Circle);
		stars.set_shape_radius(10.0);
		stars.set_emission_rate(8.0);
		stars.set_start_speed(20.0, 40.0);
		stars.set_start_lifetime(1.4, 1.8);
		stars.set_start_size(14.0, 22.0);
		stars.set_start_rotation(0.0, math::PI * 2.0);
		stars.set_angular_velocity(-2.0, 2.0);
		stars.set_size_over_lifetime(1.0, 0.6);
		stars.set_image(Some(create_star_image()));
		stars.set_render_shape(ParticleRenderShape::Image);
		let stars_id = self.add_particle_cell(root_id, 3, "image", Vector2::create(cell_center_x, 56.0));
		self.attach_particle_system(stars_id, stars);

		// 끌개: 가운데로 빨려 들며 도는 소용돌이. (더하기 합성)
		let mut vortex = ParticleSystem::new();
		vortex.set_emitter_shape(ParticleEmitterShape::Circle);
		vortex.set_shape_radius(46.0);
		vortex.set_emission_rate(140.0);
		vortex.set_start_speed(0.0, 6.0);
		vortex.set_attractor(0.0, 0.0, 260.0, 140.0);
		vortex.set_damping(1.6);
		vortex.set_start_lifetime(1.6, 2.4);
		vortex.set_start_size(2.0, 4.0);
		vortex.set_size_over_lifetime(1.0, 0.5);
		vortex.set_start_color(Color::create(0.5, 0.7, 1.0, 1.0), Color::create(0.8, 0.6, 1.0, 1.0));
		vortex.set_color_over_lifetime(vec![(0.0, Color::create(1.0, 1.0, 1.0, 0.0)), (0.15, Color::white()), (0.85, Color::create(1.0, 1.0, 1.0, 0.9)), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		vortex.set_additive(true);
		let vortex_id = self.add_particle_cell(root_id, 4, "attractor", Vector2::create(cell_center_x, 54.0));
		self.attach_particle_system(vortex_id, vortex);

		// 흔들림: 살랑이며 오르는 물방울.
		let mut bubbles = ParticleSystem::new();
		bubbles.set_emitter_shape(ParticleEmitterShape::Box);
		bubbles.set_box_size(70.0, 0.0);
		bubbles.set_emission_rate(12.0);
		bubbles.set_start_speed(25.0, 40.0);
		bubbles.set_wobble(10.0, 3.0);
		bubbles.set_start_lifetime(2.4, 3.0);
		bubbles.set_start_size(5.0, 9.0);
		bubbles.set_size_over_lifetime(0.6, 1.2);
		bubbles.set_start_color(Color::create(0.6, 0.85, 1.0, 0.75), Color::create(0.8, 0.95, 1.0, 0.75));
		let bubbles_id = self.add_particle_cell(root_id, 5, "wobble", Vector2::create(cell_center_x, 108.0));
		self.attach_particle_system(bubbles_id, bubbles);

		// edge: 위 가로 선분에서 내리는 눈.
		let mut snow = ParticleSystem::new();
		snow.set_emitter_shape(ParticleEmitterShape::Edge);
		snow.set_edge_width(110.0);
		snow.set_emission_rate(20.0);
		snow.set_start_speed(15.0, 30.0);
		snow.set_gravity(0.0, 8.0);
		snow.set_wobble(5.0, 1.4);
		snow.set_start_lifetime(4.0, 5.0);
		snow.set_start_size(3.0, 5.0);
		snow.set_start_color(Color::create(1.0, 1.0, 1.0, 0.9), Color::create(0.85, 0.92, 1.0, 0.9));
		let snow_id = self.add_particle_cell(root_id, 6, "edge", Vector2::create(cell_center_x, -4.0));
		self.attach_particle_system(snow_id, snow);

		// 월드 공간: 도는 방출점이 남긴 입자가 제자리에 남아 자국이 된다. (더하기 합성)
		let mut trail_particles = ParticleSystem::new();
		trail_particles.set_emission_rate(70.0);
		trail_particles.set_start_speed(4.0, 12.0);
		trail_particles.set_start_lifetime(0.9, 1.2);
		trail_particles.set_start_size(4.0, 7.0);
		trail_particles.set_start_color(Color::create(0.5, 1.0, 0.7, 1.0), Color::create(0.4, 0.9, 1.0, 1.0));
		trail_particles.set_color_over_lifetime(vec![(0.0, Color::white()), (1.0, Color::create(1.0, 1.0, 1.0, 0.0))]);
		trail_particles.set_world_space(true);
		trail_particles.set_additive(true);
		let orbit_center = Vector2::create(cell_center_x, 54.0);
		let orbit_particle_id = self.add_particle_cell(root_id, 7, "world space", orbit_center);
		// 도는 컴포넌트를 먼저 붙여, 파티클이 tick 에서 읽는 노드 위치가 이번 프레임 것이 되게 한다.
		self.scene_base.get_node_tree_mut()[orbit_particle_id].add_component(OrbitMover {
			component_base: ComponentBase::new(),
			center: orbit_center,
			radius: 26.0,
			angular_speed: 3.0,
			angle: 0.0,
		});
		self.attach_particle_system(orbit_particle_id, trail_particles);

		self.add_label(root_id, Vector2::create(PARTICLE_ROW_LEFT + 4.0, 575.0), Vector2::create(400.0, 16.0), "ParticleSystem: 렌더 모양 넷과 끌개, 흔들림, edge, 월드 공간", 13.0, TextAlign::Left);
	}

	//==============================================================================
	// 파티클 시간 진행. (수동 tick, 프레임마다 같은 시간)
	//==============================================================================
	fn simulate_particles(&mut self) {
		let node_tree = self.scene_base.get_node_tree_mut();
		for particle_id in self.particle_node_ids.iter() {
			if let Some(particle_system) = node_tree[*particle_id].get_component_mut::<ParticleSystem>() {
				particle_system.simulate(MOVER_FRAME_STEP);
			}
		}
	}

	//==============================================================================
	// 효과 묶음 적용. (30 프레임마다 바뀐다. 4 번은 화면 전체)
	//==============================================================================
	fn apply_effect_set(&mut self, effect_set_index: usize) {
		let screen_effect = &mut self.screen_effect;
		screen_effect.reset_all();
		self.is_full_screen_effect = false;
		let description = match effect_set_index {
			0 => {
				screen_effect.set_enabled(ScreenEffectType::Bloom, true);
				screen_effect.set_strength(ScreenEffectType::Bloom, 0.9);
				screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				"bloom + vignette"
			}
			1 => {
				screen_effect.set_enabled(ScreenEffectType::Glitch, true);
				screen_effect.set_enabled(ScreenEffectType::Chromatic, true);
				screen_effect.set_parameters(ScreenEffectType::Chromatic, 2.0, 0.0, 0.0, 0.0);
				"glitch + chromatic"
			}
			2 => {
				screen_effect.set_enabled(ScreenEffectType::Bloom, true);
				screen_effect.set_enabled(ScreenEffectType::Chromatic, true);
				screen_effect.set_strength(ScreenEffectType::Chromatic, 0.8);
				screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				screen_effect.set_enabled(ScreenEffectType::ColorGrade, true);
				screen_effect.set_parameters(ScreenEffectType::ColorGrade, 1.15, 1.2, 1.05, 0.0);
				"bloom + chromatic + vignette + colorGrade"
			}
			3 => {
				screen_effect.set_enabled(ScreenEffectType::Pixelate, true);
				screen_effect.set_strength(ScreenEffectType::Pixelate, 0.7);
				screen_effect.set_enabled(ScreenEffectType::Posterize, true);
				"pixelate + posterize"
			}
			4 => {
				self.is_full_screen_effect = true;
				screen_effect.set_enabled(ScreenEffectType::Shockwave, true);
				screen_effect.set_enabled(ScreenEffectType::Scanline, true);
				screen_effect.set_strength(ScreenEffectType::Scanline, 0.6);
				screen_effect.set_enabled(ScreenEffectType::Vignette, true);
				screen_effect.set_strength(ScreenEffectType::Vignette, 0.8);
				"화면 전체: shockwave + scanline + vignette"
			}
			5 => {
				screen_effect.set_enabled(ScreenEffectType::Blur, true);
				screen_effect.set_enabled(ScreenEffectType::ColorGrade, true);
				screen_effect.set_parameters(ScreenEffectType::ColorGrade, 1.2, 0.3, 1.05, 0.0);
				"blur + colorGrade"
			}
			6 => {
				screen_effect.set_enabled(ScreenEffectType::GodRays, true);
				screen_effect.set_parameters(ScreenEffectType::GodRays, 0.73, 0.22, 0.5, 0.94);
				screen_effect.set_enabled(ScreenEffectType::Rain, true);
				"godRays + rain"
			}
			_ => {
				screen_effect.set_enabled(ScreenEffectType::IrisWipe, true);
				screen_effect.set_strength(ScreenEffectType::IrisWipe, 0.5);
				screen_effect.set_enabled(ScreenEffectType::Halftone, true);
				screen_effect.set_strength(ScreenEffectType::Halftone, 0.6);
				"irisWipe + halftone"
			}
		};
		let effect_set_text = format!("ScreenEffect: {}", description);
		if let Some(effect_set_text_id) = self.effect_set_text_id {
			if let Some(label) = self.scene_base.get_node_tree_mut()[effect_set_text_id].get_component_mut::<Text>() {
				label.set_text(&effect_set_text);
			}
		}
	}

	//==============================================================================
	// 모든 효과 켜기. (한 프레임 동안 패스 셰이더를 다 컴파일해 본다)
	//==============================================================================
	fn enable_all_effects(&mut self) {
		for effect_type in ScreenEffect::get_effect_order() {
			self.screen_effect.set_enabled(effect_type, true);
			self.screen_effect.set_strength(effect_type, 0.5);
		}
		self.is_full_screen_effect = false;
		self.applied_effect_set_index = None;
	}

	//==============================================================================
	// 무대 영역을 캔버스 픽셀로 바꿔 ScreenEffect 영역으로. (원본 예제 apply_stage_region)
	//==============================================================================
	fn apply_stage_region(&mut self, engine: &Engine) {
		if self.is_full_screen_effect {
			self.screen_effect.clear_region();
			return;
		}
		let view_manager = engine.get_view_manager();
		let view_native_rect = view_manager.get_view_native_rect();
		let target_resolution_scale = view_manager.get_target_resolution_scale();
		let device_pixel_ratio = view_manager.get_device_pixel_ratio();
		let pixel_scale = target_resolution_scale * device_pixel_ratio;
		self.screen_effect.set_region(
			view_native_rect.position.x * device_pixel_ratio + STAGE_X * pixel_scale,
			view_native_rect.position.y * device_pixel_ratio + STAGE_Y * pixel_scale,
			STAGE_WIDTH * pixel_scale,
			STAGE_HEIGHT * pixel_scale,
			STAGE_CORNER_RADIUS * pixel_scale,
		);
	}
}

impl Scene for EffectGalleryScene {
	fn get_scene_base(&self) -> &SceneBase {
		&self.scene_base
	}

	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
		&mut self.scene_base
	}

	//==============================================================================
	// 초기화. (그림은 코드로 만든다)
	//==============================================================================
	fn initialize(&mut self, _engine: &mut Engine) {
		self.scene_base.set_scene_background_color(BACKGROUND_COLOR);
		let root_id = self.scene_base.get_root_id();
		self.scene_base.get_root_mut().set_content_size(Vector2::create(VIEW_WIDTH, VIEW_HEIGHT));
		let sheet_image = create_character_sheet();
		self.create_shader_sprite_grid(root_id, &sheet_image);
		self.create_stage(root_id, &sheet_image);
		self.create_trail_showcase(root_id);
		self.create_particle_showcase(root_id);
		self.add_label(root_id, Vector2::create(GRID_LEFT + 430.0, 26.0), Vector2::create(400.0, 18.0), "ShaderSpriteEffect 50 가지", 15.0, TextAlign::Left);
	}

	//==============================================================================
	// 갱신. (효과 묶음 바꾸기, 충격파 반지름 움직이기)
	//==============================================================================
	fn tick(&mut self, engine: &mut Engine, time_delta: f32) {
		let frame_number = engine.get_frame_number();
		if frame_number == COMPILE_CHECK_FRAME {
			self.enable_all_effects();
		}
		else {
			let effect_set_index = ((frame_number / EFFECT_SET_FRAME_COUNT) as usize) % EFFECT_SET_COUNT;
			if self.applied_effect_set_index != Some(effect_set_index) {
				self.apply_effect_set(effect_set_index);
				self.applied_effect_set_index = Some(effect_set_index);
			}
		}
		let shockwave_radius = (frame_number % EFFECT_SET_FRAME_COUNT) as f32 / EFFECT_SET_FRAME_COUNT as f32 * 1.2 + 0.1;
		let stage_center_x = (STAGE_X + STAGE_WIDTH * 0.5) / VIEW_WIDTH;
		let stage_center_y = (STAGE_Y + STAGE_HEIGHT * 0.45) / VIEW_HEIGHT;
		self.screen_effect.set_parameters(ScreenEffectType::Shockwave, stage_center_x, stage_center_y, shockwave_radius, 0.12);
		self.screen_effect.tick(time_delta);
		self.scene_base.tick(engine, time_delta);
		self.simulate_particles();
	}

	//==============================================================================
	// 이전 출력. (영역을 정하고 효과 시작, 배경)
	//==============================================================================
	fn pre_draw(&mut self, engine: &mut Engine, graphic: &mut Graphic) {
		self.apply_stage_region(engine);
		self.screen_effect.begin(graphic);
		self.scene_base.draw_scene_background(engine, graphic);
	}

	//==============================================================================
	// 이후 출력. (화면을 떠 와 효과를 건다)
	//==============================================================================
	fn post_draw(&mut self, _engine: &mut Engine, graphic: &mut Graphic) {
		self.screen_effect.end(graphic);
	}
}

//==============================================================================
// 시작.
//==============================================================================
fn main() {
	let mut engine_configuration = EngineConfiguration::new();
	engine_configuration.title = String::from("효과 갤러리 (바닐라엔진)");
	engine_configuration.auto_resize_on_window_resize = true;
	engine_configuration.reference_resolution_size = Vector2::create(VIEW_WIDTH, VIEW_HEIGHT);
	engine_configuration.view_scale_mode = ViewScaleMode::StretchShort;
	engine_configuration.use_statistics = true;

	let engine = Engine::new(engine_configuration);
	if let Err(error) = engine.run(Box::new(EffectGalleryScene::new())) {
		eprintln!("{}", error);
		std::process::exit(1);
	}
}
