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
	Simple,
	Sliced,
	Tiled,
}

// 스프라이트 출력 방식의 반사 값. (원본 SpriteDrawMode 글)
crate::core::reflection::impl_reflect_names!(SpriteDrawMode, [
	(SpriteDrawMode::Simple, "simple"),
	(SpriteDrawMode::Sliced, "sliced"),
	(SpriteDrawMode::Tiled, "tiled"),
]);

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
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> Sprite {
		let mut sprite = Sprite::new();
		sprite.set_image(Some(image));
		sprite
	}

	//==============================================================================
	/// 출력 방식 설정.
	//==============================================================================
	pub fn set_sprite_draw_mode(&mut self, sprite_draw_mode: SpriteDrawMode) {
		self.sprite_draw_mode = sprite_draw_mode;
	}

	//==============================================================================
	/// 출력 방식 반환.
	//==============================================================================
	pub fn get_sprite_draw_mode(&self) -> SpriteDrawMode {
		self.sprite_draw_mode
	}

	//==============================================================================
	/// 블렌드 모드 설정.
	//==============================================================================
	pub fn set_sprite_blend_mode(&mut self, sprite_blend_mode: SpriteBlendMode) {
		self.sprite_blend_mode = sprite_blend_mode;
	}

	//==============================================================================
	/// 블렌드 모드 반환.
	//==============================================================================
	pub fn get_sprite_blend_mode(&self) -> SpriteBlendMode {
		self.sprite_blend_mode
	}

	//==============================================================================
	/// 나인슬라이스 영역 설정. (position.x / position.y / size.x / size.y = 왼쪽 / 위 / 오른쪽 / 아래)
	//==============================================================================
	pub fn set_nine_slice(&mut self, nine_slice: Rect) {
		self.nine_slice = nine_slice;
	}

	//==============================================================================
	/// 나인슬라이스 영역 반환.
	//==============================================================================
	pub fn get_nine_slice(&self) -> Rect {
		self.nine_slice
	}

	//==============================================================================
	/// 이미지 설정.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	/// 조각 걸기. (Frame 의 그림과 그 안의 영역을 함께 건다, None 이면 그림을 비운다, 원본 setFrame, SpriteAtlas.read_frame)
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
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	/// 그림 경로 설정. (원본 sprite.imagePath, UI 문서가 적고 읽는다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	/// 그림 경로 반환. (정하지 않았으면 빈 글)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}

	//==============================================================================
	/// 이미지 영역 설정. (비우면 전체)
	//==============================================================================
	pub fn set_image_rect(&mut self, image_rect: Rect) {
		self.image_rect = image_rect;
	}

	//==============================================================================
	/// 이미지 영역 반환.
	//==============================================================================
	pub fn get_image_rect(&self) -> Rect {
		self.image_rect
	}

	//==============================================================================
	/// 좌우 뒤집기 설정.
	//==============================================================================
	pub fn set_horizontal_flip(&mut self, flip: bool) {
		self.is_horizontal_flip = flip;
	}

	//==============================================================================
	/// 상하 뒤집기 설정.
	//==============================================================================
	pub fn set_vertical_flip(&mut self, flip: bool) {
		self.is_vertical_flip = flip;
	}

	//==============================================================================
	/// 좌우 뒤집기 여부.
	//==============================================================================
	pub fn is_horizontal_flip(&self) -> bool {
		self.is_horizontal_flip
	}

	//==============================================================================
	/// 상하 뒤집기 여부.
	//==============================================================================
	pub fn is_vertical_flip(&self) -> bool {
		self.is_vertical_flip
	}

	//==============================================================================
	/// 뒤집히는 값. (뒤집힌 축은 -1)
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
	//==============================================================================
	#[reflect(skip)]
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.paint.set_color(color);
	}

	//==============================================================================
	/// 색상 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_color(&self) -> Color {
		self.paint.get_color()
	}

	//==============================================================================
	/// 둥근 모서리 크기 설정. (이미지가 없을 때 칠하는 사각형, 원본은 Paint 를 상속해 같은 함수를 쓴다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_round_size(&mut self, round_size: f32) {
		self.paint.set_round_size(round_size);
	}

	//==============================================================================
	/// 둥근 모서리 크기 반환.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_round_size(&self) -> f32 {
		self.paint.get_round_size()
	}
}

impl Default for Sprite {
	fn default() -> Sprite {
		Sprite::new()
	}
}

#[crate::reflect]
impl Component for Sprite {
	fn get_component_base(&self) -> &ComponentBase {
		self.paint.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.paint.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"Sprite"
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	/// 조각, 나인 슬라이스, 타일 모두 내용 영역 안에 그리고, 뒤집어도 영역 안에서 뒤집으므로 늘 영역 안이다.
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

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
