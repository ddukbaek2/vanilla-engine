//==============================================================================
//! 스프라이트 출력자 컴포넌트.
//! - 이미지가 없으면 Paint 처럼 색 사각형을 그린다. (원본은 Paint 를 상속, 여기서는 Paint 를 품는다)
//! - 색의 알파가 0 보다 크면 이미지 실루엣 안쪽에만 그 색을 덮는다. (틴트)
//! - 그림 경로(image_path)는 원본이 sprite.imagePath 로 덧붙여 쓰는 값이다. UI 문서(UIDocument)가 그림 대신 이 경로를 적고 읽는다. 그리기에는 쓰지 않는다.
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::frame::Frame;
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use std::rc::Rc;

/// 원본의 SpriteBlendMode 는 Graphic 의 블렌드 모드와 같은 값이다.
pub type SpriteBlendMode = BlendMode;

//==============================================================================
/// 뒤집어 그릴 때 먼저 옮길 거리. (뒤집은 축은 내용 크기만큼 옮긴 뒤 -1 배로 거울처럼 뒤집어 내용 영역 안에 남긴다)
///
/// # 인자
/// - `content_size`: 노드의 내용 크기.
/// - `flip`: 뒤집기 배율. (뒤집은 축은 -1, 아니면 1)
///
/// # 돌려줌
/// 옮길 거리. (뒤집은 축은 내용 크기, 아니면 0)
//==============================================================================
fn read_flip_offset(content_size: Vector2, flip: Vector2) -> Vector2 {
	let offset_x = if flip.x < 0.0 {
		content_size.x
	}
	else {
		0.0
	};
	let offset_y = if flip.y < 0.0 {
		content_size.y
	}
	else {
		0.0
	};
	Vector2::create(offset_x, offset_y)
}

//==============================================================================
/// 스프라이트 출력 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteDrawMode {
	/// 그림(image_rect 조각, 비면 그림 전체)을 노드 content_size 에 늘려 그린다.
	Simple,
	/// 나인슬라이스. 네 귀퉁이는 그대로 두고 변과 가운데를 늘려 그림 전체를 content_size 에 그린다. (image_rect 는 쓰지 않는다, content_size 가 왼쪽 + 오른쪽이나 위 + 아래보다 작으면 그리지 않는다)
	Sliced,
	/// image_rect 조각(비면 그림 전체)을 제 크기로 content_size 영역에 되풀이해 깔고, 영역 밖은 자른다.
	Tiled,
}

// 스프라이트 출력 방식의 반사 값. (원본 SpriteDrawMode 글)
crate::core::reflection::impl_reflect_names!(SpriteDrawMode, [
	(SpriteDrawMode::Simple, "simple"),
	(SpriteDrawMode::Sliced, "sliced"),
	(SpriteDrawMode::Tiled, "tiled"),
]);

//==============================================================================
/// 스프라이트 출력자 컴포넌트. (그림을 노드 content_size 에 그린다)
/// - 그림이 없으면 Paint 처럼 색 사각형을 칠한다. 그림이 있고 색의 알파가 0 보다 크면 그림 실루엣 안쪽을 그 색으로 덮는다. (틴트)
/// - 출력 방식(Simple, Sliced, Tiled), 블렌드 모드, 좌우 / 상하 뒤집기를 고른다.
/// - 처음은 그림 없음, 색 투명, Simple, Normal, 뒤집기 없음, 영역과 나인슬라이스는 빈 사각형.
///
/// # 예제
/// 그림을 붙여 왼쪽을 보게 뒤집기
/// ```rust
/// use vanilla::{Image, NodeTree, Sprite, Vector2};
///
/// let image = Image::create_from_pixels(16, 16, vec![255u8; 16 * 16 * 4]);
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let player_id = node_tree.create_child(root_id, "Player");
/// node_tree[player_id].set_content_size(Vector2::create(64.0, 64.0));
/// let sprite = node_tree.add_component(player_id, Sprite::create_with_image(image)).unwrap();
/// sprite.set_horizontal_flip(true);
///
/// let sprite = node_tree[player_id].get_component::<Sprite>().unwrap();
/// assert!(sprite.get_image().is_some());
/// assert_eq!(sprite.get_flip(), Vector2::create(-1.0, 1.0));
/// ```
///
/// 그림 없이 둥근 색 사각형
/// ```rust
/// use vanilla::{Color, Sprite};
///
/// let mut sprite = Sprite::new();
/// sprite.set_color("#44aa66");
/// sprite.set_round_size(8.0);
/// assert!(sprite.get_image().is_none());
/// assert_eq!(sprite.get_color(), Color::create_from_hex("#44aa66"));
/// ```
//==============================================================================
pub struct Sprite {
	paint: Paint,
	image: Option<Rc<Image>>,
	image_rect: Rect,
	is_horizontal_flip: bool,
	is_vertical_flip: bool,
	sprite_draw_mode: SpriteDrawMode,
	sprite_blend_mode: SpriteBlendMode,
	nine_slice: Rect,
	image_path: String,
}

#[crate::reflect(base = paint)]
impl Sprite {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 그림이 없고 색이 투명인 새 컴포넌트. (Simple, Normal)
	//==============================================================================
	pub fn new() -> Sprite {
		let mut paint = Paint::new();
		paint.set_color(Color::transparent());
		Sprite {
			paint,
			image: None,
			image_rect: Rect::zero(),
			is_horizontal_flip: false,
			is_vertical_flip: false,
			sprite_draw_mode: SpriteDrawMode::Simple,
			sprite_blend_mode: BlendMode::Normal,
			nine_slice: Rect::zero(),
			image_path: String::new(),
		}
	}

	//==============================================================================
	/// 이미지와 함께 생성.
	///
	/// # 인자
	/// - `image`: 그릴 그림.
	///
	/// # 돌려줌
	/// 그 그림 전체를 그리는 새 컴포넌트. (영역은 빈 사각형이라 그림 전체)
	///
	/// # 예제
	/// 그림 전체를 그리는 스프라이트
	/// ```rust
	/// use vanilla::{Image, Sprite};
	///
	/// let image = Image::create_from_pixels(32, 16, vec![255u8; 32 * 16 * 4]);
	/// let sprite = Sprite::create_with_image(image);
	/// assert_eq!(sprite.get_image().unwrap().get_width(), 32);
	/// assert!(sprite.get_image_rect().is_zero());
	/// ```
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> Sprite {
		let mut sprite = Sprite::new();
		sprite.set_image(Some(image));
		sprite
	}

	//==============================================================================
	/// 출력 방식 설정.
	///
	/// # 인자
	/// - `sprite_draw_mode`: 출력 방식. (처음 Simple)
	//==============================================================================
	pub fn set_sprite_draw_mode(&mut self, sprite_draw_mode: SpriteDrawMode) {
		self.sprite_draw_mode = sprite_draw_mode;
	}

	//==============================================================================
	/// 출력 방식 반환.
	///
	/// # 돌려줌
	/// 출력 방식.
	//==============================================================================
	pub fn get_sprite_draw_mode(&self) -> SpriteDrawMode {
		self.sprite_draw_mode
	}

	//==============================================================================
	/// 블렌드 모드 설정.
	///
	/// # 인자
	/// - `sprite_blend_mode`: 그림을 그릴 때 걸 블렌드 모드. (처음 Normal, 그린 뒤 Normal 로 되돌린다, 그림이 없을 때는 쓰지 않는다)
	//==============================================================================
	pub fn set_sprite_blend_mode(&mut self, sprite_blend_mode: SpriteBlendMode) {
		self.sprite_blend_mode = sprite_blend_mode;
	}

	//==============================================================================
	/// 블렌드 모드 반환.
	///
	/// # 돌려줌
	/// 그림을 그릴 때 거는 블렌드 모드.
	//==============================================================================
	pub fn get_sprite_blend_mode(&self) -> SpriteBlendMode {
		self.sprite_blend_mode
	}

	//==============================================================================
	/// 나인슬라이스 영역 설정. (position.x / position.y / size.x / size.y = 왼쪽 / 위 / 오른쪽 / 아래)
	///
	/// # 인자
	/// - `nine_slice`: 그림 가장자리에서 늘이지 않을 두께. (픽셀, Sliced 일 때만 쓴다)
	///
	/// # 예제
	/// 가장자리 16 픽셀을 지키는 판
	/// ```rust
	/// use vanilla::{Image, Rect, Sprite, SpriteDrawMode};
	///
	/// let panel_image = Image::create_from_pixels(48, 48, vec![255u8; 48 * 48 * 4]);
	/// let mut sprite = Sprite::create_with_image(panel_image);
	/// sprite.set_sprite_draw_mode(SpriteDrawMode::Sliced);
	/// sprite.set_nine_slice(Rect::create(16.0, 16.0, 16.0, 16.0));
	/// assert_eq!(sprite.get_sprite_draw_mode(), SpriteDrawMode::Sliced);
	/// assert_eq!(sprite.get_nine_slice().size.x, 16.0);
	/// ```
	//==============================================================================
	pub fn set_nine_slice(&mut self, nine_slice: Rect) {
		self.nine_slice = nine_slice;
	}

	//==============================================================================
	/// 나인슬라이스 영역 반환.
	///
	/// # 돌려줌
	/// 왼쪽 / 위 / 오른쪽 / 아래 두께. (position.x / position.y / size.x / size.y, 픽셀)
	//==============================================================================
	pub fn get_nine_slice(&self) -> Rect {
		self.nine_slice
	}

	//==============================================================================
	/// 이미지 설정.
	///
	/// # 인자
	/// - `image`: 그릴 그림. (None 이면 색 사각형을 칠한다, 영역은 그대로 둔다)
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	/// 조각 걸기. (Frame 의 그림과 그 안의 영역을 함께 건다, None 이면 그림을 비운다, 원본 setFrame, SpriteAtlas.read_frame)
	///
	/// # 인자
	/// - `frame`: 걸 조각. (None 이면 그림만 비우고 영역은 그대로 둔다)
	///
	/// # 예제
	/// 시트의 둘째 칸 걸기
	/// ```rust
	/// use vanilla::{Frame, Image, Rect, Sprite};
	///
	/// let sheet = Image::create_from_pixels(64, 16, vec![255u8; 64 * 16 * 4]);
	/// let frame = Frame::new(sheet, Some(Rect::create(16.0, 0.0, 16.0, 16.0)));
	/// let mut sprite = Sprite::new();
	/// sprite.set_frame(Some(&frame));
	/// assert_eq!(sprite.get_image_rect(), Rect::create(16.0, 0.0, 16.0, 16.0));
	///
	/// sprite.set_frame(None);
	/// assert!(sprite.get_image().is_none());
	/// ```
	//==============================================================================
	pub fn set_frame(&mut self, frame: Option<&Frame>) {
		match frame {
			Some(frame) => {
				let image = frame.get_image().clone();
				let image_rect = frame.get_image_rect();
				self.set_image(Some(image));
				self.set_image_rect(image_rect);
			}
			None => {
				self.set_image(None);
			}
		}
	}

	//==============================================================================
	/// 이미지 반환.
	///
	/// # 돌려줌
	/// 그리는 그림. 없으면 None.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	/// 그림 경로 설정. (원본 sprite.imagePath, UI 문서가 적고 읽는다)
	///
	/// # 인자
	/// - `image_path`: 그림 경로. (적어 두기만 하고 그림을 읽지 않는다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	/// 그림 경로 반환. (정하지 않았으면 빈 글)
	///
	/// # 돌려줌
	/// 그림 경로. (정하지 않았으면 빈 글)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}

	//==============================================================================
	/// 이미지 영역 설정. (비우면 전체)
	///
	/// # 인자
	/// - `image_rect`: 그림 안의 영역. (픽셀, 그림 왼쪽 위 기준, Rect::zero() 면 그림 전체, Sliced 는 쓰지 않는다)
	//==============================================================================
	pub fn set_image_rect(&mut self, image_rect: Rect) {
		self.image_rect = image_rect;
	}

	//==============================================================================
	/// 이미지 영역 반환.
	///
	/// # 돌려줌
	/// 그림 안의 영역. (픽셀, 빈 사각형이면 그림 전체)
	//==============================================================================
	pub fn get_image_rect(&self) -> Rect {
		self.image_rect
	}

	//==============================================================================
	/// 좌우 뒤집기 설정.
	///
	/// # 인자
	/// - `flip`: 좌우로 뒤집어 그릴지. (처음 false)
	//==============================================================================
	pub fn set_horizontal_flip(&mut self, flip: bool) {
		self.is_horizontal_flip = flip;
	}

	//==============================================================================
	/// 상하 뒤집기 설정.
	///
	/// # 인자
	/// - `flip`: 상하로 뒤집어 그릴지. (처음 false)
	//==============================================================================
	pub fn set_vertical_flip(&mut self, flip: bool) {
		self.is_vertical_flip = flip;
	}

	//==============================================================================
	/// 좌우 뒤집기 여부.
	///
	/// # 돌려줌
	/// 좌우로 뒤집어 그리면 true.
	//==============================================================================
	pub fn is_horizontal_flip(&self) -> bool {
		self.is_horizontal_flip
	}

	//==============================================================================
	/// 상하 뒤집기 여부.
	///
	/// # 돌려줌
	/// 상하로 뒤집어 그리면 true.
	//==============================================================================
	pub fn is_vertical_flip(&self) -> bool {
		self.is_vertical_flip
	}

	//==============================================================================
	/// 뒤집히는 값. (뒤집힌 축은 -1)
	///
	/// # 돌려줌
	/// 축마다 뒤집혔으면 -1, 아니면 1 인 값. (x 는 좌우, y 는 상하, 그릴 크기에 곱한다)
	///
	/// # 예제
	/// 상하만 뒤집기
	/// ```rust
	/// use vanilla::{Sprite, Vector2};
	///
	/// let mut sprite = Sprite::new();
	/// sprite.set_vertical_flip(true);
	/// assert_eq!(sprite.get_flip(), Vector2::create(1.0, -1.0));
	/// ```
	//==============================================================================
	pub fn get_flip(&self) -> Vector2 {
		let flip_x = if self.is_horizontal_flip {
			-1.0
		}
		else {
			1.0
		};
		let flip_y = if self.is_vertical_flip {
			-1.0
		}
		else {
			1.0
		};
		Vector2::create(flip_x, flip_y)
	}

	//==============================================================================
	/// 색상 설정. (이미지가 있으면 틴트, 없으면 채우기 색)
	///
	/// # 인자
	/// - `color`: 틴트 또는 채우기 색. (Color 또는 "#rrggbb" 같은 문자열, 처음 투명, 틴트는 알파가 0 보다 클 때만 건다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.paint.set_color(color);
	}

	//==============================================================================
	/// 색상 반환.
	///
	/// # 돌려줌
	/// 틴트 또는 채우기 색.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_color(&self) -> Color {
		self.paint.get_color()
	}

	//==============================================================================
	/// 둥근 모서리 크기 설정. (이미지가 없을 때 칠하는 사각형, 원본은 Paint 를 상속해 같은 함수를 쓴다)
	///
	/// # 인자
	/// - `round_size`: 귀퉁이 둥근 크기. (노드 좌표, 0 이하면 곧은 사각형, 처음 0)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_round_size(&mut self, round_size: f32) {
		self.paint.set_round_size(round_size);
	}

	//==============================================================================
	/// 둥근 모서리 크기 반환.
	///
	/// # 돌려줌
	/// 그림이 없을 때 칠하는 사각형의 귀퉁이 둥근 크기. (노드 좌표)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_round_size(&self) -> f32 {
		self.paint.get_round_size()
	}
}

impl Default for Sprite {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> Sprite {
		Sprite::new()
	}
}

#[crate::reflect]
impl Component for Sprite {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 Paint 자리, get_component::<Paint>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.paint)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.paint)
	}

	//==============================================================================
	/// 공통 상태 반환. (품은 Paint 의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.paint.get_component_base()
	}

	//==============================================================================
	/// 공통 상태 반환. (변경용, 품은 Paint 의 것)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.paint.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 이름. ("Sprite")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"Sprite"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (그리기만 하므로 false)
	//==============================================================================
	fn is_tick_needed(&self) -> bool {
		false
	}

	/// 조각, 나인 슬라이스, 타일 모두 내용 영역 안에 그리고, 뒤집어도 영역 안에서 뒤집으므로 늘 영역 안이다.
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (그림이 없으면 색 사각형, 있으면 블렌드 모드와 틴트를 걸고 출력 방식대로 그린 뒤 되돌린다)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let color = self.get_color();

		// 일반 사각형 출력.
		let image = match &self.image {
			Some(image) => {
				image.clone()
			}
			None => {
				self.paint.draw_paint(transform, graphic);
				return;
			}
		};

		let position = Vector2::zero();
		let content_size = transform.get_content_size();
		let flip = self.get_flip();
		let is_flipped = flip.x < 0.0 || flip.y < 0.0;
		let sprite_blend_mode = self.get_sprite_blend_mode();
		let sprite_draw_mode = self.get_sprite_draw_mode();

		// 슬라이스드 모드에서 컨텐츠 크기가 나인슬라이스 경계 합보다 작으면 출력 안 함.
		if sprite_draw_mode == SpriteDrawMode::Sliced {
			let nine_slice = self.get_nine_slice();
			let left = nine_slice.position.x;
			let top = nine_slice.position.y;
			let right = nine_slice.size.x;
			let bottom = nine_slice.size.y;
			if content_size.x < left + right || content_size.y < top + bottom {
				return;
			}
		}

		// 블렌드 모드 설정.
		graphic.set_blend_mode(sprite_blend_mode);

		// 컬러 틴트 설정. (알파가 0 초과일 때만, 이미지 실루엣 안쪽에만 색을 덮음)
		if color.alpha > 0.0 {
			graphic.set_image_tint_color(Some(color));
		}

		// 뒤집기는 내용 영역의 가운데를 축으로 거울처럼 뒤집는다. (그림은 내용 영역 안에 남고, 조각, 나인 슬라이스, 타일 모두 같다)
		if is_flipped {
			let flip_offset = read_flip_offset(content_size, flip);
			graphic.push_state();
			graphic.translate(flip_offset.x, flip_offset.y);
			graphic.scale(flip.x, flip.y);
		}

		// 드로우 모드에 따라 이미지 출력.
		match sprite_draw_mode {
			SpriteDrawMode::Simple => {
				let image_rect = self.get_image_rect();
				graphic.draw_image_with_image_rect(&image, position, content_size, image_rect);
			}
			SpriteDrawMode::Sliced => {
				let nine_slice = self.get_nine_slice();
				graphic.draw_image_with_nine_slice(&image, position, content_size, nine_slice);
			}
			SpriteDrawMode::Tiled => {
				let mut image_rect = self.get_image_rect();
				if image_rect.is_zero() {
					image_rect = Rect::create(0.0, 0.0, image.get_width() as f32, image.get_height() as f32);
				}

				let tile_width = image_rect.get_width().ceil().max(1.0);
				let tile_height = image_rect.get_height().ceil().max(1.0);
				let fill_width = content_size.x.abs();
				let fill_height = content_size.y.abs();

				// 클리핑 후 수동 타일 반복.
				graphic.push_state();
				graphic.translate(position.x, position.y);
				graphic.begin_clip_rect(Rect::create(0.0, 0.0, fill_width, fill_height), 0.0);
				let mut tile_y = 0.0;
				while tile_y < fill_height {
					let mut tile_x = 0.0;
					while tile_x < fill_width {
						graphic.draw_image_with_source_and_destination(&image, image_rect.position.x, image_rect.position.y, image_rect.get_width(), image_rect.get_height(), tile_x, tile_y, tile_width, tile_height);
						tile_x += tile_width;
					}
					tile_y += tile_height;
				}
				graphic.end_clip_rect();
				graphic.pop_state();
			}
		}

		if is_flipped {
			graphic.pop_state();
		}

		// 컬러 틴트 해제.
		if color.alpha > 0.0 {
			graphic.set_image_tint_color(None);
		}

		// 블렌드 모드 복원.
		graphic.set_blend_mode(BlendMode::Normal);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 뒤집어도 내용 영역의 두 모서리가 영역 안에 남는지. (옮긴 뒤 뒤집기 배율을 곱한 자리)
	//==============================================================================
	#[test]
	fn flipped_sprite_stays_inside_content() {
		let content_size = Vector2::create(120.0, 80.0);
		let flip_list = [Vector2::create(1.0, 1.0), Vector2::create(-1.0, 1.0), Vector2::create(1.0, -1.0), Vector2::create(-1.0, -1.0)];
		for flip in flip_list {
			let flip_offset = read_flip_offset(content_size, flip);
			let corner_list = [Vector2::create(0.0, 0.0), content_size];
			let mut mapped_x_list = Vec::new();
			let mut mapped_y_list = Vec::new();
			for corner in corner_list {
				let mapped_x = flip_offset.x + corner.x * flip.x;
				let mapped_y = flip_offset.y + corner.y * flip.y;
				assert!(mapped_x >= 0.0 && mapped_x <= content_size.x);
				assert!(mapped_y >= 0.0 && mapped_y <= content_size.y);
				mapped_x_list.push(mapped_x);
				mapped_y_list.push(mapped_y);
			}
			// 뒤집은 축은 왼쪽 위 모서리가 오른쪽 아래로 간다.
			if flip.x < 0.0 {
				assert_eq!(mapped_x_list[0], content_size.x);
			}
			if flip.y < 0.0 {
				assert_eq!(mapped_y_list[0], content_size.y);
			}
		}
	}

	//==============================================================================
	/// 뒤집어도 내용 영역 안에서만 그린다고 알리는지. (화면 밖 거르기가 내용 영역으로 본다)
	//==============================================================================
	#[test]
	fn flipped_sprite_is_drawn_inside_content() {
		let mut sprite = Sprite::new();
		sprite.set_horizontal_flip(true);
		sprite.set_vertical_flip(true);
		assert!(sprite.is_drawn_inside_content());
	}
}
