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
	simple,
	sliced,
	tiled,
}

pub struct Sprite {
	paint: Paint,
	image: Option<Rc<Image>>,
	imageRect: Rect,
	isHorizontalFlip: bool,
	isVerticalFlip: bool,
	spriteDrawMode: SpriteDrawMode,
	spriteBlendMode: SpriteBlendMode,
	nineSlice: Rect,
}

impl Sprite {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> Sprite {
		let mut paint = Paint::new();
		paint.setColor(Color::transparent());
		Sprite {
			paint,
			image: None,
			imageRect: Rect::zero(),
			isHorizontalFlip: false,
			isVerticalFlip: false,
			spriteDrawMode: SpriteDrawMode::simple,
			spriteBlendMode: BlendMode::normal,
			nineSlice: Rect::zero(),
		}
	}

	//==============================================================================
	// 이미지와 함께 생성.
	//==============================================================================
	pub fn createWithImage(image: Rc<Image>) -> Sprite {
		let mut sprite = Sprite::new();
		sprite.setImage(Some(image));
		sprite
	}

	//==============================================================================
	// 출력 방식 설정.
	//==============================================================================
	pub fn setSpriteDrawMode(&mut self, spriteDrawMode: SpriteDrawMode) {
		self.spriteDrawMode = spriteDrawMode;
	}

	//==============================================================================
	// 출력 방식 반환.
	//==============================================================================
	pub fn getSpriteDrawMode(&self) -> SpriteDrawMode {
		self.spriteDrawMode
	}

	//==============================================================================
	// 블렌드 모드 설정.
	//==============================================================================
	pub fn setSpriteBlendMode(&mut self, spriteBlendMode: SpriteBlendMode) {
		self.spriteBlendMode = spriteBlendMode;
	}

	//==============================================================================
	// 블렌드 모드 반환.
	//==============================================================================
	pub fn getSpriteBlendMode(&self) -> SpriteBlendMode {
		self.spriteBlendMode
	}

	//==============================================================================
	// 나인슬라이스 영역 설정. (position.x / position.y / size.x / size.y = 왼쪽 / 위 / 오른쪽 / 아래)
	//==============================================================================
	pub fn setNineSlice(&mut self, nineSlice: Rect) {
		self.nineSlice = nineSlice;
	}

	//==============================================================================
	// 나인슬라이스 영역 반환.
	//==============================================================================
	pub fn getNineSlice(&self) -> Rect {
		self.nineSlice
	}

	//==============================================================================
	// 이미지 설정.
	//==============================================================================
	pub fn setImage(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn getImage(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	// 이미지 영역 설정. (비우면 전체)
	//==============================================================================
	pub fn setImageRect(&mut self, imageRect: Rect) {
		self.imageRect = imageRect;
	}

	//==============================================================================
	// 이미지 영역 반환.
	//==============================================================================
	pub fn getImageRect(&self) -> Rect {
		self.imageRect
	}

	//==============================================================================
	// 좌우 뒤집기 설정.
	//==============================================================================
	pub fn setHorizontalFlip(&mut self, flip: bool) {
		self.isHorizontalFlip = flip;
	}

	//==============================================================================
	// 상하 뒤집기 설정.
	//==============================================================================
	pub fn setVerticalFlip(&mut self, flip: bool) {
		self.isVerticalFlip = flip;
	}

	//==============================================================================
	// 좌우 뒤집기 여부.
	//==============================================================================
	pub fn isHorizontalFlip(&self) -> bool {
		self.isHorizontalFlip
	}

	//==============================================================================
	// 상하 뒤집기 여부.
	//==============================================================================
	pub fn isVerticalFlip(&self) -> bool {
		self.isVerticalFlip
	}

	//==============================================================================
	// 뒤집히는 값. (뒤집힌 축은 -1)
	//==============================================================================
	pub fn getFlip(&self) -> Vector2 {
		let flipX = if self.isHorizontalFlip {
			-1.0
		}
		else {
			1.0
		};
		let flipY = if self.isVerticalFlip {
			-1.0
		}
		else {
			1.0
		};
		Vector2::create(flipX, flipY)
	}

	//==============================================================================
	// 색상 설정. (이미지가 있으면 틴트, 없으면 채우기 색)
	//==============================================================================
	pub fn setColor(&mut self, color: impl Into<Color>) {
		self.paint.setColor(color);
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn getColor(&self) -> Color {
		self.paint.getColor()
	}
}

impl Default for Sprite {
	fn default() -> Sprite {
		Sprite::new()
	}
}

impl Component for Sprite {
	fn getComponentBase(&self) -> &ComponentBase {
		self.paint.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.paint.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"Sprite"
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	// 뒤집으면 contentSize 반대쪽으로 그려질 수 있어서, 뒤집지 않았을 때만 영역 안으로 친다.
	fn isDrawnInsideContent(&self) -> bool {
		let flip = self.getFlip();
		flip.x >= 0.0 && flip.y >= 0.0
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let color = self.getColor();

		// 일반 사각형 출력.
		let image = match &self.image {
			Some(image) => {
				image.clone()
			}
			None => {
				self.paint.drawPaint(transform, graphic);
				return;
			}
		};

		let position = Vector2::zero();
		let contentSize = transform.getContentSize();
		let flip = self.getFlip();
		let imageSize = contentSize.multiply(flip);
		let spriteBlendMode = self.getSpriteBlendMode();
		let spriteDrawMode = self.getSpriteDrawMode();

		// 슬라이스드 모드에서 컨텐츠 크기가 나인슬라이스 경계 합보다 작으면 출력 안 함.
		if spriteDrawMode == SpriteDrawMode::sliced {
			let nineSlice = self.getNineSlice();
			let left = nineSlice.position.x;
			let top = nineSlice.position.y;
			let right = nineSlice.size.x;
			let bottom = nineSlice.size.y;
			if contentSize.x < left + right || contentSize.y < top + bottom {
				return;
			}
		}

		// 블렌드 모드 설정.
		graphic.setBlendMode(spriteBlendMode);

		// 컬러 틴트 설정. (알파가 0 초과일 때만, 이미지 실루엣 안쪽에만 색을 덮음)
		if color.alpha > 0.0 {
			graphic.setImageTintColor(Some(color));
		}

		// 드로우 모드에 따라 이미지 출력.
		match spriteDrawMode {
			SpriteDrawMode::simple => {
				let imageRect = self.getImageRect();
				graphic.drawImageWithImageRect(&image, position, imageSize, imageRect);
			}
			SpriteDrawMode::sliced => {
				let nineSlice = self.getNineSlice();
				graphic.drawImageWithNineSlice(&image, position, imageSize, nineSlice);
			}
			SpriteDrawMode::tiled => {
				let mut imageRect = self.getImageRect();
				if imageRect.isZero() {
					imageRect = Rect::create(0.0, 0.0, image.getWidth() as f32, image.getHeight() as f32);
				}

				let tileWidth = imageRect.getWidth().ceil().max(1.0);
				let tileHeight = imageRect.getHeight().ceil().max(1.0);
				let fillWidth = imageSize.x.abs();
				let fillHeight = imageSize.y.abs();

				// 클리핑 후 수동 타일 반복.
				graphic.pushState();
				graphic.translate(position.x, position.y);
				graphic.beginClipRect(Rect::create(0.0, 0.0, fillWidth, fillHeight), 0.0);
				let mut tileY = 0.0;
				while tileY < fillHeight {
					let mut tileX = 0.0;
					while tileX < fillWidth {
						graphic.drawImageWithSourceAndDestination(&image, imageRect.position.x, imageRect.position.y, imageRect.getWidth(), imageRect.getHeight(), tileX, tileY, tileWidth, tileHeight);
						tileX += tileWidth;
					}
					tileY += tileHeight;
				}
				graphic.endClipRect();
				graphic.popState();
			}
		}

		// 컬러 틴트 해제.
		if color.alpha > 0.0 {
			graphic.setImageTintColor(None);
		}

		// 블렌드 모드 복원.
		graphic.setBlendMode(BlendMode::normal);
	}
}
