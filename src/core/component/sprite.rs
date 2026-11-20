//==============================================================================
// 스프라이트 출력자 컴포넌트.
// - 이미지가 없으면 Paint 처럼 색 사각형을 그린다. (원본은 Paint 를 상속, 여기서는 Paint 를 품는다)
// - 색의 알파가 0 보다 크면 이미지 실루엣 안쪽에만 그 색을 덮는다. (틴트)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::component::paint::Paint;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::{BlendMode, Graphic};
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use std::rc::Rc;

// 원본의 SpriteBlendMode 는 Graphic 의 블렌드 모드와 같은 값이다.
pub type SpriteBlendMode = BlendMode;

//==============================================================================
// 스프라이트 출력 방식.
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpriteDrawMode {
	Simple,
	Sliced,
	Tiled,
}

pub struct Sprite {
	paint: Paint,
	image: Option<Rc<Image>>,
	image_rect: Rect,
	is_horizontal_flip: bool,
	is_vertical_flip: bool,
	sprite_draw_mode: SpriteDrawMode,
	sprite_blend_mode: SpriteBlendMode,
	nine_slice: Rect,
}

impl Sprite {
	//==============================================================================
	// 생성.
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
		}
	}

	//==============================================================================
	// 이미지와 함께 생성.
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> Sprite {
		let mut sprite = Sprite::new();
		sprite.set_image(Some(image));
		sprite
	}

	//==============================================================================
	// 출력 방식 설정.
	//==============================================================================
	pub fn set_sprite_draw_mode(&mut self, sprite_draw_mode: SpriteDrawMode) {
		self.sprite_draw_mode = sprite_draw_mode;
	}

	//==============================================================================
	// 출력 방식 반환.
	//==============================================================================
	pub fn get_sprite_draw_mode(&self) -> SpriteDrawMode {
		self.sprite_draw_mode
	}

	//==============================================================================
	// 블렌드 모드 설정.
	//==============================================================================
	pub fn set_sprite_blend_mode(&mut self, sprite_blend_mode: SpriteBlendMode) {
		self.sprite_blend_mode = sprite_blend_mode;
	}

	//==============================================================================
	// 블렌드 모드 반환.
	//==============================================================================
	pub fn get_sprite_blend_mode(&self) -> SpriteBlendMode {
		self.sprite_blend_mode
	}

	//==============================================================================
	// 나인슬라이스 영역 설정. (position.x / position.y / size.x / size.y = 왼쪽 / 위 / 오른쪽 / 아래)
	//==============================================================================
	pub fn set_nine_slice(&mut self, nine_slice: Rect) {
		self.nine_slice = nine_slice;
	}

	//==============================================================================
	// 나인슬라이스 영역 반환.
	//==============================================================================
	pub fn get_nine_slice(&self) -> Rect {
		self.nine_slice
	}

	//==============================================================================
	// 이미지 설정.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	// 이미지 영역 설정. (비우면 전체)
	//==============================================================================
	pub fn set_image_rect(&mut self, image_rect: Rect) {
		self.image_rect = image_rect;
	}

	//==============================================================================
	// 이미지 영역 반환.
	//==============================================================================
	pub fn get_image_rect(&self) -> Rect {
		self.image_rect
	}

	//==============================================================================
	// 좌우 뒤집기 설정.
	//==============================================================================
	pub fn set_horizontal_flip(&mut self, flip: bool) {
		self.is_horizontal_flip = flip;
	}

	//==============================================================================
	// 상하 뒤집기 설정.
	//==============================================================================
	pub fn set_vertical_flip(&mut self, flip: bool) {
		self.is_vertical_flip = flip;
	}

	//==============================================================================
	// 좌우 뒤집기 여부.
	//==============================================================================
	pub fn is_horizontal_flip(&self) -> bool {
		self.is_horizontal_flip
	}

	//==============================================================================
	// 상하 뒤집기 여부.
	//==============================================================================
	pub fn is_vertical_flip(&self) -> bool {
		self.is_vertical_flip
	}

	//==============================================================================
	// 뒤집히는 값. (뒤집힌 축은 -1)
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
	// 색상 설정. (이미지가 있으면 틴트, 없으면 채우기 색)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.paint.set_color(color);
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.paint.get_color()
	}
}

impl Default for Sprite {
	fn default() -> Sprite {
		Sprite::new()
	}
}

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

	// 뒤집으면 content_size 반대쪽으로 그려질 수 있어서, 뒤집지 않았을 때만 영역 안으로 친다.
	fn is_drawn_inside_content(&self) -> bool {
		let flip = self.get_flip();
		flip.x >= 0.0 && flip.y >= 0.0
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
		let image_size = content_size.multiply(flip);
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

		// 드로우 모드에 따라 이미지 출력.
		match sprite_draw_mode {
			SpriteDrawMode::Simple => {
				let image_rect = self.get_image_rect();
				graphic.draw_image_with_image_rect(&image, position, image_size, image_rect);
			}
			SpriteDrawMode::Sliced => {
				let nine_slice = self.get_nine_slice();
				graphic.draw_image_with_nine_slice(&image, position, image_size, nine_slice);
			}
			SpriteDrawMode::Tiled => {
				let mut image_rect = self.get_image_rect();
				if image_rect.is_zero() {
					image_rect = Rect::create(0.0, 0.0, image.get_width() as f32, image.get_height() as f32);
				}

				let tile_width = image_rect.get_width().ceil().max(1.0);
				let tile_height = image_rect.get_height().ceil().max(1.0);
				let fill_width = image_size.x.abs();
				let fill_height = image_size.y.abs();

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

		// 컬러 틴트 해제.
		if color.alpha > 0.0 {
			graphic.set_image_tint_color(None);
		}

		// 블렌드 모드 복원.
		graphic.set_blend_mode(BlendMode::Normal);
	}
}
