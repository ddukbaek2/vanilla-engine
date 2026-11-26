//==============================================================================
//! 2D 뼈대 애니메이터 컴포넌트. (원본 experimental/skeleton2d/skeleton2danimator.js, 노드에 붙여 뼈대를 돌리고 그린다)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - Skeleton2D(자세)와 Skeleton2DAnimationState(트랙)를 들고, tick 에서 시간을 흘려 자세를 쓰고, draw 에서 월드 행렬과 IK 를 셈해 그린다.
//!   draw 에서 셈하므로 씬의 tick 이 컴포넌트 tick 뒤에 옮긴 뼈(IK 목표 같은)도 그 프레임에 걸린다.
//! - 그림은 붙임의 조각 이름으로 찾는다. set_atlas 가 엔진 SpriteAtlas 에서 모든 스킨의 조각을 찾아 담고, set_frame 으로 하나씩 넣을 수도 있다.
//! - 슬롯마다 그리기 차례대로 삼각형을 Graphic 의 정점 자리(get_vertex_data, x, y, u, v)에 적어 draw_vertices 로 그린다. 슬롯 색은 그림에 곱한다.
//!   같은 쪽 그림의 슬롯은 엔진의 묶어 그리기가 한 번에 그린다. Graphic 은 고치지 않았다.
//! - 뼈대 원점은 노드의 축점 자리다. (내용 크기가 0 이면 노드 자리)
//! - SpriteAnimator(프레임 애니메이션)와는 따로다. 그것은 같은 노드의 Sprite 그림을 바꾸고, 이것은 스스로 그린다.
//! - 기즈모를 켜면(set_gizmo_visible) 뼈를 선으로 그린다.
//! - 원본과 다른 점: 자료는 `Rc<Skeleton2DData>` 로 받는다. 조각은 Frame 을 복사해 든다. 컴포넌트 밖에서 시간을 흘릴 때는 tick 대신 update 를 부른다.
//!
//! # 예제
//! 노드에 뼈대를 붙여 걷기를 틀고 IK 목표 옮기기
//! ```rust
//! use std::rc::Rc;
//! use vanilla::experimental::skeleton2d::{Skeleton2DAnimator, Skeleton2DData};
//! use vanilla::NodeTree;
//!
//! let json = serde_json::json!({
//! 	"bones": [{ "name": "root" }, { "name": "leg", "parent": "root", "rotation": 90, "length": 30 }, { "name": "target", "parent": "root", "x": 20, "y": 25 }],
//! 	"ik": [{ "name": "legIk", "bones": ["leg"], "target": "target" }],
//! 	"animations": { "walk": { "bones": { "root": { "translate": [{ "time": 0, "y": 0 }, { "time": 0.5, "y": -4 }, { "time": 1, "y": 0 }] } } } }
//! });
//! let mut node_tree = NodeTree::new();
//! let root_id = node_tree.get_root_id();
//! let hero_id = node_tree.create_child(root_id, "hero");
//! let skeleton_animator = node_tree[hero_id].add_component(Skeleton2DAnimator::new());
//! skeleton_animator.set_skeleton_data(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
//! skeleton_animator.set_animation(0, "walk", true, 0.0);
//! skeleton_animator.update(0.25);
//! skeleton_animator.find_bone_mut("target").expect("target").set_position(-20.0, 25.0);
//! let skeleton = skeleton_animator.get_skeleton_mut().expect("skeleton");
//! skeleton.update_world_transform();
//! assert_eq!(skeleton.find_bone("root").expect("root").get_y(), -2.0);
//! ```
//==============================================================================
use crate::base::color::Color;
use crate::base::vector2::Vector2;
use crate::core::component::{Component, ComponentBase};
use crate::core::frame::Frame;
use crate::core::graphic::{Graphic, FLOATS_PER_VERTEX};
use crate::core::node::nodetree::ComponentContext;
use crate::core::node::worldnode::NodeTransform;
use crate::experimental::skeleton2d::skeleton2d::{Skeleton2D, Skeleton2DBone};
use crate::experimental::skeleton2d::skeleton2danimationstate::{Skeleton2DAnimationState, Skeleton2DFiredEvent};
use crate::experimental::skeleton2d::skeleton2ddata::Skeleton2DData;
use crate::resource::spriteatlas::SpriteAtlas;
use std::collections::HashMap;
use std::rc::Rc;

/// 기즈모 뼈 선 색.
const GIZMO_BONE_COLOR: Color = Color::create(0.3, 0.9, 1.0, 1.0);

/// 기즈모 관절 색.
const GIZMO_JOINT_COLOR: Color = Color::create(1.0, 0.8, 0.2, 1.0);

//==============================================================================
/// 2D 뼈대 애니메이터 컴포넌트.
///
/// # 예제
/// 아틀라스를 걸고 두 트랙을 섞기 (그리기는 엔진이 노드를 그릴 때)
/// ```rust
/// use std::rc::Rc;
/// use vanilla::experimental::skeleton2d::{Skeleton2DAnimator, Skeleton2DData};
/// use vanilla::{Frame, Image, Rect};
///
/// let json = serde_json::json!({
/// 	"bones": [{ "name": "root" }, { "name": "body", "parent": "root", "y": -50 }],
/// 	"slots": [{ "name": "body", "bone": "body", "attachment": "body" }],
/// 	"skins": [{ "name": "default", "attachments": { "body": { "body": { "type": "region", "frame": "body", "width": 100, "height": 100 } } } }],
/// 	"animations": {
/// 		"bob": { "bones": { "body": { "translate": [{ "time": 0, "y": 0 }, { "time": 0.5, "y": -10 }, { "time": 1, "y": 0 }] } } },
/// 		"spin": { "bones": { "body": { "rotate": [{ "time": 0, "value": 0 }, { "time": 2, "value": 360 }] } } }
/// 	}
/// });
/// let mut skeleton_animator = Skeleton2DAnimator::new();
/// skeleton_animator.set_skeleton_data(Rc::new(Skeleton2DData::create_from_json(&json).expect("skeleton")));
/// let image = Image::create_from_pixels(4, 4, vec![255u8; 4 * 4 * 4]);
/// skeleton_animator.set_frame("body", Frame::new(image, Some(Rect::create(0.0, 0.0, 4.0, 4.0))));
/// assert!(skeleton_animator.set_animation(0, "bob", true, 0.0));
/// assert!(skeleton_animator.set_animation(1, "spin", true, 0.0));
/// skeleton_animator.set_track_alpha(1, 0.5);
/// skeleton_animator.update(0.5);
/// assert!(skeleton_animator.get_frame("body").is_some());
/// assert_eq!(skeleton_animator.find_bone("body").expect("body").get_rotation(), 45.0);
/// ```
//==============================================================================
pub struct Skeleton2DAnimator {
	component_base: ComponentBase,
	skeleton_data: Option<Rc<Skeleton2DData>>,
	skeleton: Option<Skeleton2D>,
	animation_state: Option<Skeleton2DAnimationState>,
	frame_table: HashMap<String, Frame>,
	time_scale: f64,
	vertex_positions: Vec<f64>,
}

impl Skeleton2DAnimator {
	//==============================================================================
	/// 생성. (자료 없음, 빠르기 1)
	///
	/// # 돌려줌
	/// 애니메이터 컴포넌트.
	//==============================================================================
	pub fn new() -> Skeleton2DAnimator {
		Skeleton2DAnimator {
			component_base: ComponentBase::new(),
			skeleton_data: None,
			skeleton: None,
			animation_state: None,
			frame_table: HashMap::new(),
			time_scale: 1.0,
			vertex_positions: vec![0.0f64; 64],
		}
	}

	//==============================================================================
	/// 뼈대 자료 설정. (새 Skeleton2D 와 Skeleton2DAnimationState 를 짓는다, 트랙은 비운다)
	///
	/// # 인자
	/// - `skeleton_data`: 뼈대 자료.
	//==============================================================================
	pub fn set_skeleton_data(&mut self, skeleton_data: Rc<Skeleton2DData>) {
		let mut skeleton = Skeleton2D::new(skeleton_data.clone());
		skeleton.update_world_transform();
		self.animation_state = Some(Skeleton2DAnimationState::new(skeleton_data.clone()));
		self.skeleton = Some(skeleton);
		self.skeleton_data = Some(skeleton_data);
	}

	//==============================================================================
	/// 뼈대 자료 반환.
	///
	/// # 돌려줌
	/// 뼈대 자료. (없으면 None)
	//==============================================================================
	pub fn get_skeleton_data(&self) -> Option<&Rc<Skeleton2DData>> {
		self.skeleton_data.as_ref()
	}

	//==============================================================================
	/// 뼈대 반환.
	///
	/// # 돌려줌
	/// 뼈대. (자료가 없으면 None)
	//==============================================================================
	pub fn get_skeleton(&self) -> Option<&Skeleton2D> {
		self.skeleton.as_ref()
	}

	//==============================================================================
	/// 뼈대 반환. (변경용)
	///
	/// # 돌려줌
	/// 뼈대. (자료가 없으면 None)
	//==============================================================================
	pub fn get_skeleton_mut(&mut self) -> Option<&mut Skeleton2D> {
		self.skeleton.as_mut()
	}

	//==============================================================================
	/// 애니메이션 상태 반환.
	///
	/// # 돌려줌
	/// 애니메이션 상태. (자료가 없으면 None)
	//==============================================================================
	pub fn get_animation_state(&self) -> Option<&Skeleton2DAnimationState> {
		self.animation_state.as_ref()
	}

	//==============================================================================
	/// 애니메이션 상태 반환. (변경용)
	///
	/// # 돌려줌
	/// 애니메이션 상태. (자료가 없으면 None)
	//==============================================================================
	pub fn get_animation_state_mut(&mut self) -> Option<&mut Skeleton2DAnimationState> {
		self.animation_state.as_mut()
	}

	//==============================================================================
	/// 아틀라스에서 조각 찾아 담기. (모든 스킨의 붙임 조각 이름으로)
	/// - 아틀라스를 다 읽은 뒤에 부른다. 자료를 바꾸면 다시 부른다.
	///
	/// # 인자
	/// - `sprite_atlas`: 엔진 SpriteAtlas.
	///
	/// # 돌려줌
	/// 붙임의 조각을 모두 찾았으면 true. (자료가 없으면 false)
	//==============================================================================
	pub fn set_atlas(&mut self, sprite_atlas: &SpriteAtlas) -> bool {
		let Some(skeleton_data) = self.skeleton_data.clone() else {
			return false;
		};
		let mut is_all_found = true;
		for skin in skeleton_data.get_skins() {
			for attachment in skin.get_attachment_list() {
				let frame_name = attachment.get_frame_name();
				match sprite_atlas.read_frame(frame_name) {
					Some(frame) => {
						self.set_frame(frame_name, frame.clone());
					}
					None => {
						is_all_found = false;
					}
				}
			}
		}
		is_all_found
	}

	//==============================================================================
	/// 조각 하나 넣기. (같은 이름이 있으면 바꾼다)
	///
	/// # 인자
	/// - `frame_name`: 붙임의 조각 이름.
	/// - `frame`: 그림과 영역.
	//==============================================================================
	pub fn set_frame(&mut self, frame_name: &str, frame: Frame) {
		self.frame_table.insert(frame_name.to_string(), frame);
	}

	//==============================================================================
	/// 조각 찾기.
	///
	/// # 인자
	/// - `frame_name`: 조각 이름.
	///
	/// # 돌려줌
	/// 조각. (없으면 None)
	//==============================================================================
	pub fn get_frame(&self, frame_name: &str) -> Option<&Frame> {
		self.frame_table.get(frame_name)
	}

	//==============================================================================
	/// 트랙에 클립 걸기. (Skeleton2DAnimationState::set_animation)
	///
	/// # 인자
	/// - `track_index`: 트랙 번호. (0 이 바탕)
	/// - `animation_name`: 클립 이름.
	/// - `is_loop`: 되풀이하면 true.
	/// - `mix_duration`: 섞는 시간. (초)
	///
	/// # 돌려줌
	/// 걸었으면 true. (자료가 없거나 클립이 없으면 false)
	//==============================================================================
	pub fn set_animation(&mut self, track_index: usize, animation_name: &str, is_loop: bool, mix_duration: f64) -> bool {
		match self.animation_state.as_mut() {
			Some(animation_state) => {
				animation_state.set_animation(track_index, animation_name, is_loop, mix_duration)
			}
			None => {
				false
			}
		}
	}

	//==============================================================================
	/// 트랙의 층 몫 설정.
	///
	/// # 인자
	/// - `track_index`: 트랙 번호.
	/// - `alpha`: 층 몫. (0 ~ 1)
	//==============================================================================
	pub fn set_track_alpha(&mut self, track_index: usize, alpha: f64) {
		if let Some(animation_state) = self.animation_state.as_mut() {
			animation_state.set_track_alpha(track_index, alpha);
		}
	}

	//==============================================================================
	/// 뼈 찾기.
	///
	/// # 인자
	/// - `bone_name`: 뼈 이름.
	///
	/// # 돌려줌
	/// 뼈. (없거나 자료가 없으면 None)
	//==============================================================================
	pub fn find_bone(&self, bone_name: &str) -> Option<&Skeleton2DBone> {
		self.skeleton.as_ref().and_then(|skeleton| skeleton.find_bone(bone_name))
	}

	//==============================================================================
	/// 뼈 찾기. (변경용)
	///
	/// # 인자
	/// - `bone_name`: 뼈 이름.
	///
	/// # 돌려줌
	/// 뼈. (없거나 자료가 없으면 None)
	//==============================================================================
	pub fn find_bone_mut(&mut self, bone_name: &str) -> Option<&mut Skeleton2DBone> {
		self.skeleton.as_mut().and_then(|skeleton| skeleton.find_bone_mut(bone_name))
	}

	//==============================================================================
	/// 빠르기 설정. (tick 이 흘리는 시간에 곱한다, 0 이면 멈춘다)
	///
	/// # 인자
	/// - `time_scale`: 빠르기.
	//==============================================================================
	pub fn set_time_scale(&mut self, time_scale: f64) {
		self.time_scale = time_scale;
	}

	//==============================================================================
	/// 빠르기 반환.
	///
	/// # 돌려줌
	/// 빠르기.
	//==============================================================================
	pub fn get_time_scale(&self) -> f64 {
		self.time_scale
	}

	//==============================================================================
	/// 시간을 흘려 자세 쓰기. (빠르기를 곱하지 않는다, 정해진 걸음으로 돌릴 때 직접 부른다)
	///
	/// # 인자
	/// - `time_delta`: 흐른 시간. (초)
	//==============================================================================
	pub fn update(&mut self, time_delta: f64) {
		if let (Some(animation_state), Some(skeleton)) = (self.animation_state.as_mut(), self.skeleton.as_mut()) {
			animation_state.update(time_delta);
			animation_state.apply(skeleton);
		}
	}

	//==============================================================================
	/// 모은 사건 꺼내기.
	///
	/// # 돌려줌
	/// 알린 사건 목록. (자료가 없으면 빈 목록)
	//==============================================================================
	pub fn take_events(&mut self) -> Vec<Skeleton2DFiredEvent> {
		match self.animation_state.as_mut() {
			Some(animation_state) => {
				animation_state.take_events()
			}
			None => {
				Vec::new()
			}
		}
	}

	//==============================================================================
	/// 뼈대 원점 읽기. (노드 내용 영역 안의 축점 자리, 원본처럼 f64 로 곱함)
	//==============================================================================
	fn read_origin(transform: &NodeTransform) -> (f64, f64) {
		let pivot = transform.get_pivot();
		let content_size = transform.get_content_size();
		(content_size.x as f64 * pivot.x as f64, content_size.y as f64 * pivot.y as f64)
	}
}

impl Default for Skeleton2DAnimator {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Skeleton2DAnimator {
		Skeleton2DAnimator::new()
	}
}

impl Component for Skeleton2DAnimator {
	//==============================================================================
	/// 공통 상태 반환.
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		&self.component_base
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		&mut self.component_base
	}

	//==============================================================================
	/// 컴포넌트 이름.
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Skeleton2DAnimator"
	}

	//==============================================================================
	/// 갱신. (빠르기를 곱한 시간으로 update)
	//==============================================================================
	fn tick(&mut self, _context: &mut ComponentContext, time_delta: f32) {
		let time_scale = self.time_scale;
		self.update(time_delta as f64 * time_scale);
	}

	//==============================================================================
	/// 출력. (월드 행렬과 IK 를 셈하고 그리기 차례대로 슬롯을 그린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let Some(skeleton) = self.skeleton.as_mut() else {
			return;
		};
		skeleton.update_world_transform();
		let skeleton = &*skeleton;
		let origin = Self::read_origin(transform);
		let maximum_triangle_count = graphic.get_vertex_data().len() / (FLOATS_PER_VERTEX * 3);
		let previous_blend_mode = graphic.get_blend_mode();
		for &slot_index in skeleton.get_draw_order() {
			let Some(attachment) = skeleton.get_slot_attachment(slot_index) else {
				continue;
			};
			let slot = &skeleton.get_slots()[slot_index];
			let slot_color = slot.get_color();
			if slot_color.alpha <= 0.0 {
				continue;
			}
			let Some(frame) = self.frame_table.get(attachment.get_frame_name()) else {
				continue;
			};
			let image = frame.get_image();
			let Some(texture) = graphic.get_image_texture(image) else {
				continue;
			};
			let image_rect = frame.get_image_rect();
			let vertex_count = attachment.get_vertex_count();
			if self.vertex_positions.len() < vertex_count * 2 {
				self.vertex_positions.resize(vertex_count * 2, 0.0);
			}
			skeleton.compute_attachment_vertices(slot_index, &mut self.vertex_positions, image_rect.size.x as f64, image_rect.size.y as f64);
			let uvs = attachment.get_uvs();
			let triangles = attachment.get_triangles();
			let image_width = image.get_width() as f64;
			let image_height = image.get_height() as f64;
			graphic.set_blend_mode(slot.get_data().get_blend_mode());

			// 삼각형을 정점 자리에 담을 만큼씩 나눠 그린다.
			let triangle_count = triangles.len() / 3;
			let mut first_triangle = 0usize;
			while first_triangle < triangle_count {
				let chunk_triangle_count = maximum_triangle_count.min(triangle_count - first_triangle);
				{
					let vertex_data = graphic.get_vertex_data();
					let mut offset = 0usize;
					for &vertex_index in &triangles[first_triangle * 3..(first_triangle + chunk_triangle_count) * 3] {
						vertex_data[offset] = (self.vertex_positions[vertex_index * 2] + origin.0) as f32;
						vertex_data[offset + 1] = (self.vertex_positions[vertex_index * 2 + 1] + origin.1) as f32;
						vertex_data[offset + 2] = ((image_rect.position.x as f64 + uvs[vertex_index * 2] * image_rect.size.x as f64) / image_width) as f32;
						vertex_data[offset + 3] = ((image_rect.position.y as f64 + uvs[vertex_index * 2 + 1] * image_rect.size.y as f64) / image_height) as f32;
						offset += FLOATS_PER_VERTEX;
					}
				}
				graphic.draw_vertices(chunk_triangle_count * 3, texture, slot_color);
				first_triangle += chunk_triangle_count;
			}
		}
		graphic.set_blend_mode(previous_blend_mode);
	}

	//==============================================================================
	/// 기즈모 출력. (뼈마다 원점에서 길이만큼 선, 원점에 점)
	//==============================================================================
	fn draw_gizmos(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		if !self.component_base.is_gizmo_visible() {
			return;
		}
		let Some(skeleton) = self.skeleton.as_ref() else {
			return;
		};
		let origin = Self::read_origin(transform);
		graphic.set_stroke_color(GIZMO_BONE_COLOR);
		graphic.set_fill_color(GIZMO_JOINT_COLOR);
		for bone in skeleton.get_bones() {
			let length = bone.get_data().get_length();
			let start = Vector2::create((bone.get_world_x() + origin.0) as f32, (bone.get_world_y() + origin.1) as f32);
			if length > 0.0 {
				let tip = bone.local_to_world(length, 0.0);
				graphic.draw_line(&[start, Vector2::create(tip.x + origin.0 as f32, tip.y + origin.1 as f32)], 2.0);
			}
			graphic.draw_circle(start, 3.0);
		}
	}
}
