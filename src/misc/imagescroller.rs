//==============================================================================
// 이미지 스크롤러.
// - 원본 misc/imagescroller.js 를 옮긴 것이다. 이미지 한 장을 보이는 영역(viewRect) 안에 타일처럼 깔고, 속도에 맞춰 흘려 그린다. (무한 배경)
// - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽이 tick 과 draw 를 불러 준다.
// - 원본과 다른 점:
//   이미지는 Rc<Image> 다. 원본 setImage 가 받던 ImageAsset 은 setImageAsset 으로 넘긴다.
//   getScrollPosition / getScrollSpeed 는 원본처럼 고칠 수 있는 참조가 아니라 값을 돌려준다. (tick 은 안에서 바로 더한다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::resource::imageasset::ImageAsset;
use std::rc::Rc;

//==============================================================================
// 이미지 스크롤러.
//==============================================================================
#[derive(Clone, Default)]
pub struct ImageScroller {
	image: Option<Rc<Image>>,
	viewRect: Rect,
	scrollPosition: Vector2,
	scrollSpeed: Vector2,
}

impl ImageScroller {
	//==============================================================================
	// 생성. (이미지 없음, 영역 0, 멈춤)
	//==============================================================================
	pub fn new() -> ImageScroller {
		ImageScroller {
			image: None,
			viewRect: Rect::zero(),
			scrollPosition: Vector2::zero(),
			scrollSpeed: Vector2::zero(),
		}
	}

	//==============================================================================
	// 갱신. (속도만큼 스크롤 위치를 옮긴다)
	//==============================================================================
	pub fn tick(&mut self, timeDelta: f32) {
		self.scrollPosition.x += self.scrollSpeed.x * timeDelta;
		self.scrollPosition.y += self.scrollSpeed.y * timeDelta;
	}

	//==============================================================================
	// 첫 타일의 시작 좌표. (스크롤 위치를 이미지 크기로 나눈 나머지만큼 당긴다. 좌표는 정수로 내려 틈을 막는다)
	//==============================================================================
	pub fn calculateTileStart(&self, imageWidth: f32, imageHeight: f32) -> Vector2 {
		let remainderX = (self.scrollPosition.x % imageWidth).floor();
		let remainderY = (self.scrollPosition.y % imageHeight).floor();
		let startX = if remainderX <= 0.0 {
			remainderX
		}
		else {
			remainderX - imageWidth
		};
		let startY = if remainderY <= 0.0 {
			remainderY
		}
		else {
			remainderY - imageHeight
		};
		Vector2::create(self.viewRect.position.x + startX, self.viewRect.position.y + startY)
	}

	//==============================================================================
	// 출력. (보이는 영역으로 자르고, 1.5 픽셀씩 겹쳐 그려 타일 사이 틈을 가린다)
	//==============================================================================
	pub fn draw(&self, graphic: &mut Graphic) {
		let image = match &self.image {
			Some(image) => {
				image
			}
			None => {
				return;
			}
		};
		let imageWidth = image.getWidth() as f32;
		let imageHeight = image.getHeight() as f32;
		if imageWidth == 0.0 || imageHeight == 0.0 {
			return;
		}

		let tileStart = self.calculateTileStart(imageWidth, imageHeight);
		let viewRect = self.getViewRect();
		let viewRight = viewRect.position.x + viewRect.size.x;
		let viewBottom = viewRect.position.y + viewRect.size.y;
		graphic.beginClipRect(viewRect, 0.0);

		let overlap = 1.5;
		let tileSize = Vector2::create(imageWidth + overlap, imageHeight + overlap);
		let mut tileX = tileStart.x;
		while tileX < viewRight {
			let mut tileY = tileStart.y;
			while tileY < viewBottom {
				let tilePosition = Vector2::create(tileX.floor(), tileY.floor());
				graphic.drawImage(image, tilePosition, tileSize);
				tileY += imageHeight;
			}
			tileX += imageWidth;
		}

		graphic.endClipRect();
	}

	//==============================================================================
	// 이미지 설정. (None 이면 그리지 않는다)
	//==============================================================================
	pub fn setImage(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	// 이미지 애셋의 이미지로 설정. (아직 읽히지 않았으면 None)
	//==============================================================================
	pub fn setImageAsset(&mut self, imageAsset: &mut ImageAsset) {
		self.image = imageAsset.getImage();
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn getImage(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	// 보이는 영역 설정. (부른 쪽 좌표계)
	//==============================================================================
	pub fn setViewRect(&mut self, rect: Rect) {
		self.viewRect = rect;
	}

	//==============================================================================
	// 보이는 영역 반환.
	//==============================================================================
	pub fn getViewRect(&self) -> Rect {
		self.viewRect
	}

	//==============================================================================
	// 세로 스크롤 속도 설정. (초당 픽셀)
	//==============================================================================
	pub fn setVerticalScrollSpeed(&mut self, speed: f32) {
		self.scrollSpeed.y = speed;
	}

	//==============================================================================
	// 가로 스크롤 속도 설정. (초당 픽셀)
	//==============================================================================
	pub fn setHorizontalScrollSpeed(&mut self, speed: f32) {
		self.scrollSpeed.x = speed;
	}

	//==============================================================================
	// 스크롤 속도 반환.
	//==============================================================================
	pub fn getScrollSpeed(&self) -> Vector2 {
		self.scrollSpeed
	}

	//==============================================================================
	// 스크롤 위치 설정.
	//==============================================================================
	pub fn setScrollPosition(&mut self, position: Vector2) {
		self.scrollPosition = position;
	}

	//==============================================================================
	// 스크롤 위치 반환.
	//==============================================================================
	pub fn getScrollPosition(&self) -> Vector2 {
		self.scrollPosition
	}
}

//==============================================================================
// 시험. (그리기 없이 타일 시작 계산만)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 스크롤 위치에 따라 첫 타일이 영역 왼쪽 위나 그보다 앞에서 시작한다.
	//==============================================================================
	#[test]
	fn tileStartFollowsScroll() {
		let mut imageScroller = ImageScroller::new();
		imageScroller.setViewRect(Rect::create(10.0, 20.0, 300.0, 200.0));
		imageScroller.setHorizontalScrollSpeed(-50.0);
		imageScroller.setVerticalScrollSpeed(30.0);
		imageScroller.tick(1.0);
		assert_eq!(imageScroller.getScrollPosition(), Vector2::create(-50.0, 30.0));
		// x: -50 % 100 = -50 → 그대로, y: 30 % 100 = 30 → 30 - 100.
		assert_eq!(imageScroller.calculateTileStart(100.0, 100.0), Vector2::create(-40.0, -50.0));
		imageScroller.setScrollPosition(Vector2::create(250.5, 0.0));
		assert_eq!(imageScroller.calculateTileStart(100.0, 100.0), Vector2::create(10.0 + 50.0 - 100.0, 20.0));
		assert!(imageScroller.getImage().is_none());
	}
}
