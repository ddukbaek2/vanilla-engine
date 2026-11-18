//==============================================================================
// 이미지 영역. (프레임)
// - 원본 core/frame.js 를 옮긴 것이다. 프레임 루프가 아니라 스프라이트 이미지 한 장, 또는 시트 안의 한 칸이다.
//   Animation 이 이것을 목록으로 들고 차례로 넘긴다.
// - 원본은 image 가 null 이면 예외를 던진다. 러스트 판은 Rc<Image> 를 받아서 비어 있을 수 없다.
// - 원본처럼 영역을 이미지 크기 안으로 자르지 않는다. (원본도 그 줄은 주석으로 꺼 두었다)
//==============================================================================
use crate::base::rect::Rect;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
// 이미지 영역.
//==============================================================================
#[derive(Clone)]
pub struct Frame {
	image: Rc<Image>,
	rect: Rect,
}

impl Frame {
	//==============================================================================
	// 생성. (rect 가 없으면 이미지 전체를 영역으로 한다)
	//==============================================================================
	pub fn new(image: Rc<Image>, rect: Option<Rect>) -> Frame {
		let rect = match rect {
			Some(rect) => {
				rect
			}
			None => {
				let imageWidth = image.getWidth() as f32;
				let imageHeight = image.getHeight() as f32;
				Rect::create(0.0, 0.0, imageWidth, imageHeight)
			}
		};
		Frame {
			image,
			rect,
		}
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn getImage(&self) -> &Rc<Image> {
		&self.image
	}

	//==============================================================================
	// 이미지 내부 영역 반환.
	//==============================================================================
	pub fn getImageRect(&self) -> Rect {
		self.rect
	}
}

//==============================================================================
// 시험. (GL 없이 돈다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 시험용 빈 이미지를 만든다.
	//==============================================================================
	fn createTestImage(width: u32, height: u32) -> Rc<Image> {
		let pixels = vec![0u8; (width * height * 4) as usize];
		Image::createFromPixels(width, height, pixels)
	}

	//==============================================================================
	// 영역을 주지 않으면 이미지 전체가 영역이 된다.
	//==============================================================================
	#[test]
	fn frameWithoutRectCoversWholeImage() {
		let image = createTestImage(64, 32);
		let frame = Frame::new(image.clone(), None);
		let imageRect = frame.getImageRect();
		assert_eq!(imageRect, Rect::create(0.0, 0.0, 64.0, 32.0));
		assert!(Rc::ptr_eq(frame.getImage(), &image));
	}

	//==============================================================================
	// 영역을 주면 자르지 않고 그대로 쓴다. (이미지 밖으로 나가도)
	//==============================================================================
	#[test]
	fn frameWithRectKeepsRect() {
		let image = createTestImage(64, 32);
		let sheetRect = Rect::create(16.0, 8.0, 16.0, 16.0);
		let frame = Frame::new(image.clone(), Some(sheetRect));
		assert_eq!(frame.getImageRect(), sheetRect);

		let outsideRect = Rect::create(48.0, 16.0, 32.0, 32.0);
		let outsideFrame = Frame::new(image, Some(outsideRect));
		assert_eq!(outsideFrame.getImageRect(), outsideRect);
	}
}
