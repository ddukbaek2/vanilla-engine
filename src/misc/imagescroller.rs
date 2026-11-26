//==============================================================================
//! 이미지 스크롤러.
//! - 원본 misc/imagescroller.js 를 옮긴 것이다. 이미지 한 장을 보이는 영역(view_rect) 안에 타일처럼 깔고, 속도에 맞춰 흘려 그린다. (무한 배경)
//! - 원본처럼 컴포넌트가 아니라 그냥 값이다. 들고 있는 쪽이 tick 과 draw 를 불러 준다.
//! - 원본과 다른 점:
//!   이미지는 `Rc<Image>` 다. 원본 setImage 가 받던 ImageAsset 은 set_image_asset 으로 넘긴다.
//!   get_scroll_position / get_scroll_speed 는 원본처럼 고칠 수 있는 참조가 아니라 값을 돌려준다. (tick 은 안에서 바로 더한다)
//==============================================================================
use crate::base::rect::Rect;
use crate::base::vector2::Vector2;
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::resource::imageasset::ImageAsset;
use std::rc::Rc;

//==============================================================================
/// 이미지 스크롤러.
/// - 스크롤 위치가 늘면 그림은 오른쪽(가로), 아래(세로)로 흘러간다.
///
/// # 예제
/// 왼쪽으로 흐르는 배경의 첫 타일 자리
/// ```rust
/// use vanilla::{Image, ImageScroller, Rect, Vector2};
///
/// let mut image_scroller = ImageScroller::new();
/// image_scroller.set_image(Some(Image::create_from_pixels(100, 100, vec![255u8; 100 * 100 * 4])));
/// image_scroller.set_view_rect(Rect::create(10.0, 20.0, 300.0, 200.0));
/// image_scroller.set_horizontal_scroll_speed(-50.0);
/// image_scroller.tick(1.0);
/// assert_eq!(image_scroller.get_scroll_position(), Vector2::create(-50.0, 0.0));
/// assert_eq!(image_scroller.calculate_tile_start(100.0, 100.0), Vector2::create(-40.0, 20.0));
/// ```
///
/// 씬에서 배경 흘려 그리기
/// ```rust,no_run
/// use vanilla::{Engine, Graphic, ImageScroller, Rect};
///
/// fn create_background(engine: &Engine) -> ImageScroller {
/// 	let mut image_scroller = ImageScroller::new();
/// 	image_scroller.set_image(engine.load_image("examples/assets/atlas/icons.png").ok());
/// 	image_scroller.set_view_rect(Rect::create(0.0, 0.0, 1280.0, 720.0));
/// 	image_scroller.set_vertical_scroll_speed(40.0);
/// 	image_scroller
/// }
///
/// fn draw_background(image_scroller: &mut ImageScroller, graphic: &mut Graphic, time_delta: f32) {
/// 	image_scroller.tick(time_delta);
/// 	image_scroller.draw(graphic);
/// }
/// ```
//==============================================================================
#[derive(Clone, Default)]
pub struct ImageScroller {
	image: Option<Rc<Image>>,
	view_rect: Rect,
	scroll_position: Vector2,
	scroll_speed: Vector2,
}

impl ImageScroller {
	//==============================================================================
	/// 생성. (이미지 없음, 영역 0, 멈춤)
	///
	/// # 돌려줌
	/// 이미지가 없고 스크롤 위치와 속도가 0 인 스크롤러.
	//==============================================================================
	pub fn new() -> ImageScroller {
		ImageScroller {
			image: None,
			view_rect: Rect::zero(),
			scroll_position: Vector2::zero(),
			scroll_speed: Vector2::zero(),
		}
	}

	//==============================================================================
	/// 갱신. (속도만큼 스크롤 위치를 옮긴다)
	///
	/// # 인자
	/// - `time_delta`: 지난 프레임부터 흐른 시간. (초)
	//==============================================================================
	pub fn tick(&mut self, time_delta: f32) {
		self.scroll_position.x += self.scroll_speed.x * time_delta;
		self.scroll_position.y += self.scroll_speed.y * time_delta;
	}

	//==============================================================================
	/// 첫 타일의 시작 좌표. (스크롤 위치를 이미지 크기로 나눈 나머지만큼 당긴다. 좌표는 정수로 내려 틈을 막는다)
	///
	/// # 인자
	/// - `image_width`: 이미지 너비. (픽셀, 0 이면 나머지가 NaN 이다)
	/// - `image_height`: 이미지 높이. (픽셀, 0 이면 나머지가 NaN 이다)
	///
	/// # 돌려줌
	/// 첫 타일의 왼쪽 위 좌표. (보이는 영역 왼쪽 위와 같거나 이미지 한 장 안쪽만큼 앞)
	//==============================================================================
	pub fn calculate_tile_start(&self, image_width: f32, image_height: f32) -> Vector2 {
		let remainder_x = (self.scroll_position.x % image_width).floor();
		let remainder_y = (self.scroll_position.y % image_height).floor();
		let start_x = if remainder_x <= 0.0 {
			remainder_x
		}
		else {
			remainder_x - image_width
		};
		let start_y = if remainder_y <= 0.0 {
			remainder_y
		}
		else {
			remainder_y - image_height
		};
		Vector2::create(self.view_rect.position.x + start_x, self.view_rect.position.y + start_y)
	}

	//==============================================================================
	/// 출력. (보이는 영역으로 자르고, 1.5 픽셀씩 겹쳐 그려 타일 사이 틈을 가린다)
	/// - 이미지가 없거나 너비, 높이가 0 이면 그리지 않는다.
	///
	/// # 인자
	/// - `graphic`: 그릴 그래픽. (보이는 영역과 같은 좌표계)
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
		let image_width = image.get_width() as f32;
		let image_height = image.get_height() as f32;
		if image_width == 0.0 || image_height == 0.0 {
			return;
		}

		let tile_start = self.calculate_tile_start(image_width, image_height);
		let view_rect = self.get_view_rect();
		let view_right = view_rect.position.x + view_rect.size.x;
		let view_bottom = view_rect.position.y + view_rect.size.y;
		graphic.begin_clip_rect(view_rect, 0.0);

		let overlap = 1.5;
		let tile_size = Vector2::create(image_width + overlap, image_height + overlap);
		let mut tile_x = tile_start.x;
		while tile_x < view_right {
			let mut tile_y = tile_start.y;
			while tile_y < view_bottom {
				let tile_position = Vector2::create(tile_x.floor(), tile_y.floor());
				graphic.draw_image(image, tile_position, tile_size);
				tile_y += image_height;
			}
			tile_x += image_width;
		}

		graphic.end_clip_rect();
	}

	//==============================================================================
	/// 이미지 설정. (None 이면 그리지 않는다)
	///
	/// # 인자
	/// - `image`: 깔 이미지. None 이면 그리지 않는다.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.image = image;
	}

	//==============================================================================
	/// 이미지 애셋의 이미지로 설정. (아직 읽히지 않았으면 None)
	/// - 부를 때의 이미지를 한 번 가져온다. 나중에 애셋이 다 읽혀도 저절로 바뀌지 않는다.
	///
	/// # 인자
	/// - `image_asset`: 이미지를 꺼낼 애셋.
	//==============================================================================
	pub fn set_image_asset(&mut self, image_asset: &mut ImageAsset) {
		self.image = image_asset.get_image();
	}

	//==============================================================================
	/// 이미지 반환.
	///
	/// # 돌려줌
	/// 깔린 이미지의 참조. 없으면 None.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	/// 보이는 영역 설정. (부른 쪽 좌표계)
	///
	/// # 인자
	/// - `rect`: 타일을 깔고 자를 영역.
	//==============================================================================
	pub fn set_view_rect(&mut self, rect: Rect) {
		self.view_rect = rect;
	}

	//==============================================================================
	/// 보이는 영역 반환.
	///
	/// # 돌려줌
	/// 타일을 깔고 자를 영역.
	//==============================================================================
	pub fn get_view_rect(&self) -> Rect {
		self.view_rect
	}

	//==============================================================================
	/// 세로 스크롤 속도 설정. (초당 픽셀)
	///
	/// # 인자
	/// - `speed`: 세로 속도. (초당 픽셀, + 면 그림이 아래로 흐른다)
	//==============================================================================
	pub fn set_vertical_scroll_speed(&mut self, speed: f32) {
		self.scroll_speed.y = speed;
	}

	//==============================================================================
	/// 가로 스크롤 속도 설정. (초당 픽셀)
	///
	/// # 인자
	/// - `speed`: 가로 속도. (초당 픽셀, + 면 그림이 오른쪽으로 흐른다)
	//==============================================================================
	pub fn set_horizontal_scroll_speed(&mut self, speed: f32) {
		self.scroll_speed.x = speed;
	}

	//==============================================================================
	/// 스크롤 속도 반환.
	///
	/// # 돌려줌
	/// 가로(x), 세로(y) 속도. (초당 픽셀, 복사본)
	//==============================================================================
	pub fn get_scroll_speed(&self) -> Vector2 {
		self.scroll_speed
	}

	//==============================================================================
	/// 스크롤 위치 설정.
	///
	/// # 인자
	/// - `position`: 스크롤 위치. (픽셀, 이미지 크기로 나눈 나머지만 그림에 보인다)
	//==============================================================================
	pub fn set_scroll_position(&mut self, position: Vector2) {
		self.scroll_position = position;
	}

	//==============================================================================
	/// 스크롤 위치 반환.
	///
	/// # 돌려줌
	/// 지금 스크롤 위치. (픽셀, 복사본)
	//==============================================================================
	pub fn get_scroll_position(&self) -> Vector2 {
		self.scroll_position
	}
}

//==============================================================================
/// 시험. (그리기 없이 타일 시작 계산만)
//==============================================================================
#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 스크롤 위치에 따라 첫 타일이 영역 왼쪽 위나 그보다 앞에서 시작한다.
	//==============================================================================
	#[test]
	fn tile_start_follows_scroll() {
		let mut image_scroller = ImageScroller::new();
		image_scroller.set_view_rect(Rect::create(10.0, 20.0, 300.0, 200.0));
		image_scroller.set_horizontal_scroll_speed(-50.0);
		image_scroller.set_vertical_scroll_speed(30.0);
		image_scroller.tick(1.0);
		assert_eq!(image_scroller.get_scroll_position(), Vector2::create(-50.0, 30.0));
		// x: -50 % 100 = -50 → 그대로, y: 30 % 100 = 30 → 30 - 100.
		assert_eq!(image_scroller.calculate_tile_start(100.0, 100.0), Vector2::create(-40.0, -50.0));
		image_scroller.set_scroll_position(Vector2::create(250.5, 0.0));
		assert_eq!(image_scroller.calculate_tile_start(100.0, 100.0), Vector2::create(10.0 + 50.0 - 100.0, 20.0));
		assert!(image_scroller.get_image().is_none());
	}

	//==============================================================================
	/// 이미지 애셋을 넘기면 그 애셋의 이미지를 쥔다. (원본 setImage(ImageAsset) 자리, 읽기 전 애셋이면 None)
	//==============================================================================
	#[test]
	fn image_asset_gives_its_image() {
		let mut image_scroller = ImageScroller::new();
		let mut empty_image_asset = ImageAsset::new();
		image_scroller.set_image_asset(&mut empty_image_asset);
		assert!(image_scroller.get_image().is_none());
		let mut png_bytes: Vec<u8> = Vec::new();
		image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).expect("png");
		let mut image_asset = ImageAsset::new();
		let _load_handle = image_asset.load_from_bytes("red.png", png_bytes);
		image_scroller.set_image_asset(&mut image_asset);
		let image = image_scroller.get_image().expect("image").clone();
		assert_eq!((image.get_width(), image.get_height()), (3, 2));
		assert!(Rc::ptr_eq(&image, &image_asset.get_image().expect("asset image")));
		image_scroller.set_image(None);
		assert!(image_scroller.get_image().is_none());
	}
}
