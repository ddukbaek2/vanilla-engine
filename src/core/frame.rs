//==============================================================================
//! 이미지 영역. (프레임)
//! - 원본 core/frame.js 를 옮긴 것이다. 프레임 루프가 아니라 스프라이트 이미지 한 장, 또는 시트 안의 한 칸이다.
//!   Animation 이 이것을 목록으로 들고 차례로 넘긴다.
//! - 원본은 image 가 null 이면 예외를 던진다. 러스트 판은 `Rc<Image>` 를 받아서 비어 있을 수 없다.
//! - 원본처럼 영역을 이미지 크기 안으로 자르지 않는다. (원본도 그 줄은 주석으로 꺼 두었다)
//!
//! # 예제
//! 시트 한 장을 네 칸 프레임으로 나누기
//! ```rust
//! use vanilla::{Frame, Image, Rect};
//!
//! let image = Image::create_from_pixels(64, 16, vec![0u8; 64 * 16 * 4]);
//! let mut frame_list = Vec::new();
//! for index in 0..4 {
//! 	let cell_rect = Rect::create(index as f64 * 16.0, 0.0, 16.0, 16.0);
//! 	frame_list.push(Frame::new(image.clone(), Some(cell_rect)));
//! }
//! assert_eq!(frame_list.len(), 4);
//! assert_eq!(frame_list[3].get_image_rect(), Rect::create(48.0, 0.0, 16.0, 16.0));
//! ```
//==============================================================================
use crate::base::rect::Rect;
use crate::core::image::Image;
use std::rc::Rc;

//==============================================================================
/// 이미지 영역.
/// - 이미지 한 장과 그 안의 사각형 영역(픽셀)을 같이 든다. 복제하면 이미지는 `Rc` 로 같이 쓴다.
///
/// # 예제
/// 이미지 전체 프레임과 한 칸 프레임
/// ```rust
/// use std::rc::Rc;
/// use vanilla::{Frame, Image, Rect};
///
/// let image = Image::create_from_pixels(64, 32, vec![0u8; 64 * 32 * 4]);
/// let whole_frame = Frame::new(image.clone(), None);
/// assert_eq!(whole_frame.get_image_rect(), Rect::create(0.0, 0.0, 64.0, 32.0));
/// assert!(Rc::ptr_eq(whole_frame.get_image(), &image));
///
/// let cell_frame = Frame::new(image, Some(Rect::create(16.0, 8.0, 16.0, 16.0)));
/// assert_eq!(cell_frame.get_image_rect(), Rect::create(16.0, 8.0, 16.0, 16.0));
/// ```
//==============================================================================
#[derive(Clone)]
pub struct Frame {
	image: Rc<Image>,
	rect: Rect,
}

impl Frame {
	//==============================================================================
	/// 생성. (rect 가 없으면 이미지 전체를 영역으로 한다)
	///
	/// # 인자
	/// - `image`: 영역을 뗄 이미지.
	/// - `rect`: 이미지 안의 영역. (픽셀, 이미지 왼쪽 위 기준) None 이면 (0, 0, 이미지 너비, 이미지 높이). 이미지 밖으로 나가도 자르지 않는다.
	///
	/// # 돌려줌
	/// 새 프레임.
	//==============================================================================
	pub fn new(image: Rc<Image>, rect: Option<Rect>) -> Frame {
		let rect = match rect {
			Some(rect) => {
				rect
			}
			None => {
				let image_width = image.get_width() as f64;
				let image_height = image.get_height() as f64;
				Rect::create(0.0, 0.0, image_width, image_height)
			}
		};
		Frame {
			image,
			rect,
		}
	}

	//==============================================================================
	/// 이미지 반환.
	///
	/// # 돌려줌
	/// 프레임이 든 이미지의 참조.
	//==============================================================================
	pub fn get_image(&self) -> &Rc<Image> {
		&self.image
	}

	//==============================================================================
	/// 이미지 내부 영역 반환.
	///
	/// # 돌려줌
	/// 이미지 안의 영역. (픽셀, 복사본)
	//==============================================================================
	pub fn get_image_rect(&self) -> Rect {
		self.rect
	}
}

//==============================================================================
/// 시험. (GL 없이 돈다)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 시험용 빈 이미지를 만든다.
	//==============================================================================
	fn create_test_image(width: u32, height: u32) -> Rc<Image> {
		let pixels = vec![0u8; (width * height * 4) as usize];
		Image::create_from_pixels(width, height, pixels)
	}

	//==============================================================================
	/// 영역을 주지 않으면 이미지 전체가 영역이 된다.
	//==============================================================================
	#[test]
	fn frame_without_rect_covers_whole_image() {
		let image = create_test_image(64, 32);
		let frame = Frame::new(image.clone(), None);
		let image_rect = frame.get_image_rect();
		assert_eq!(image_rect, Rect::create(0.0, 0.0, 64.0, 32.0));
		assert!(Rc::ptr_eq(frame.get_image(), &image));
	}

	//==============================================================================
	/// 영역을 주면 자르지 않고 그대로 쓴다. (이미지 밖으로 나가도)
	//==============================================================================
	#[test]
	fn frame_with_rect_keeps_rect() {
		let image = create_test_image(64, 32);
		let sheet_rect = Rect::create(16.0, 8.0, 16.0, 16.0);
		let frame = Frame::new(image.clone(), Some(sheet_rect));
		assert_eq!(frame.get_image_rect(), sheet_rect);

		let outside_rect = Rect::create(48.0, 16.0, 32.0, 32.0);
		let outside_frame = Frame::new(image, Some(outside_rect));
		assert_eq!(outside_frame.get_image_rect(), outside_rect);
	}
}
