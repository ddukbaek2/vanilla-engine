//==============================================================================
//! 이미지. (원본 HTMLImageElement / ImageAsset 자리)
//! - 형식: 원본은 브라우저가 푸는 형식을 모두 받는다. 데스크탑은 image 크레이트로 PNG, JPEG, WebP, GIF(첫 장), BMP, ICO, resvg 로 SVG,
//!   HEIF 상자는 엔진이 읽고 AV1 은 rav1d 로 풀어 AVIF 를 받는다. (core/avifimage.rs)
//!   웹은 애셋을 받을 때 브라우저(HTMLImageElement)가 먼저 풀어 손잡이에 둔다. (platform/web.rs, AVIF 와 애니메이션 PNG 의 첫 장 포함)
//! - 형(MIME)을 아는 바이트(data: 주소)는 load_from_memory_with_mime_type 으로 푼다. 브라우저처럼 형이 image/svg+xml 일 때만 SVG 이고, 그 밖은 내용으로 가린다.
//!   형을 모르는 바이트(파일)는 load_from_memory 가 내용으로 SVG 까지 가린다. (데스크탑에는 응답의 형이 없다)
//! - sniff_image_mime_type: WHATWG MIME 스니핑의 그림 표(ICO, CUR, BMP, GIF, WebP, PNG, JPEG)에 AVIF 를 더해 내용으로 형을 고른다. (데스크탑 Material 이 응답의 형 대신 쓴다)
//! - 픽셀은 프리멀티플라이드 알파 RGBA 로 들고 있다. (원본은 텍스처에 올릴 때 UNPACK_PREMULTIPLY_ALPHA 로 같은 일을 한다)
//! - Rc 로 나눠 쓰고, GPU 텍스처는 Graphic 의 ImageTextureCache 가 처음 그릴 때 만든다.
//! - GPU 에 올린 뒤에는 CPU 픽셀을 놓아 메모리를 돌려준다. (원본은 브라우저가 HTMLImageElement 를 들고 있다)
//!   픽셀을 다시 읽어야 하는 그림은 set_pixels_retained(true) 로 남긴다. (ShadowImage 는 그림을 받을 때 스스로 남기고, 이미 놓였으면 텍스처에서 되읽는다)
//!   이 엔진은 GL 컨텍스트를 잃었다 되살리는 길이 없어서 놓은 픽셀로 텍스처를 다시 올릴 일은 없다.
//==============================================================================
use crate::base::identifier;
use std::cell::{Cell, Ref, RefCell};
use std::path::Path;
use std::rc::Rc;

pub struct Image {
	instance_id: u64,
	width: u32,
	height: u32,
	/// 프리멀티플라이드 RGBA. (GPU 에 올린 뒤 놓았으면 비어 있다)
	pixels: RefCell<Vec<u8>>,
	/// GPU 에 올린 뒤에도 CPU 픽셀을 남길지.
	is_pixels_retained: Cell<bool>,
}

impl Image {
	//==============================================================================
	/// 파일에서 읽기. (load_from_memory 의 형식, 데스크탑은 읽은 파일을 리소스 사용량에 적는다)
	//==============================================================================
	pub fn load(path: &Path) -> Result<Rc<Image>, String> {
		let bytes = std::fs::read(path).map_err(|error| format!("이미지 읽기 실패 ({}): {}", path.display(), error))?;
		#[cfg(not(target_arch = "wasm32"))]
		{
			crate::base::platform::record_loaded_file(&path.to_string_lossy(), bytes.len() as u64);
		}
		Image::load_from_memory(&bytes)
	}

	//==============================================================================
	/// 메모리에서 읽기. (PNG, JPEG, WebP, GIF 의 첫 장, BMP, ICO. 데스크탑은 SVG 와 AVIF 도)
	/// - 웹은 애셋을 받을 때 브라우저가 먼저 풀어 손잡이에 둔다. (platform/web.rs, 브라우저가 푸는 형식 모두) 여기는 그 밖의 바이트를 푼다.
	//==============================================================================
	pub fn load_from_memory(bytes: &[u8]) -> Result<Rc<Image>, String> {
		#[cfg(not(target_arch = "wasm32"))]
		{
			if is_svg_bytes(bytes) {
				return decode_svg(bytes);
			}
		}
		Image::load_raster_from_memory(bytes)
	}

	//==============================================================================
	/// 형을 아는 바이트 읽기. (브라우저처럼 형이 image/svg+xml 이면 SVG, 그 밖은 내용으로 가린 래스터. data: 주소의 그림)
	/// - 웹은 SVG 를 브라우저가 푼다. (여기서는 Err)
	//==============================================================================
	pub fn load_from_memory_with_mime_type(bytes: &[u8], mime_type: &str) -> Result<Rc<Image>, String> {
		let mime_essence = mime_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
		if mime_essence == "image/svg+xml" {
			#[cfg(not(target_arch = "wasm32"))]
			{
				return decode_svg(bytes);
			}
			#[cfg(target_arch = "wasm32")]
			{
				return Err(String::from("웹의 SVG 는 브라우저가 푼다."));
			}
		}
		Image::load_raster_from_memory(bytes)
	}

	//==============================================================================
	/// 래스터 그림 읽기. (데스크탑 AVIF 와 image 크레이트의 형식, 내용으로 가린다)
	//==============================================================================
	fn load_raster_from_memory(bytes: &[u8]) -> Result<Rc<Image>, String> {
		#[cfg(not(target_arch = "wasm32"))]
		{
			if is_avif_bytes(bytes) {
				return crate::core::avifimage::decode_avif(bytes);
			}
		}
		let dynamic_image = image::load_from_memory(bytes).map_err(|error| format!("이미지 해석 실패: {}", error))?;
		let rgba_image = dynamic_image.to_rgba8();
		let width = rgba_image.width();
		let height = rgba_image.height();
		let straight_pixels = rgba_image.into_raw();
		Ok(Image::create_from_pixels(width, height, straight_pixels))
	}

	//==============================================================================
	/// 프리멀티플라이드 RGBA 픽셀로 생성. (SVG 를 그린 판처럼 이미 알파를 곱한 픽셀)
	//==============================================================================
	pub fn create_from_premultiplied_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Rc<Image> {
		Rc::new(Image {
			instance_id: identifier::create_instance_id(),
			width,
			height,
			pixels: RefCell::new(pixels),
			is_pixels_retained: Cell::new(false),
		})
	}

	//==============================================================================
	/// 스트레이트 알파 RGBA 픽셀로 생성. (코드로 만든 그림)
	//==============================================================================
	pub fn create_from_pixels(width: u32, height: u32, straight_pixels: Vec<u8>) -> Rc<Image> {
		let mut pixels = straight_pixels;
		for pixel in pixels.as_chunks_mut::<4>().0 {
			let alpha = pixel[3] as u32;
			pixel[0] = ((pixel[0] as u32 * alpha + 127) / 255) as u8;
			pixel[1] = ((pixel[1] as u32 * alpha + 127) / 255) as u8;
			pixel[2] = ((pixel[2] as u32 * alpha + 127) / 255) as u8;
		}
		Rc::new(Image {
			instance_id: identifier::create_instance_id(),
			width,
			height,
			pixels: RefCell::new(pixels),
			is_pixels_retained: Cell::new(false),
		})
	}

	//==============================================================================
	/// 인스턴스 식별자.
	//==============================================================================
	pub fn get_instance_id(&self) -> u64 {
		self.instance_id
	}

	//==============================================================================
	/// 너비. (픽셀)
	//==============================================================================
	pub fn get_width(&self) -> u32 {
		self.width
	}

	//==============================================================================
	/// 높이. (픽셀)
	//==============================================================================
	pub fn get_height(&self) -> u32 {
		self.height
	}

	//==============================================================================
	/// 프리멀티플라이드 RGBA 픽셀. (GPU 에 올린 뒤 놓았으면 빈 배열, has_pixels 로 가린다)
	//==============================================================================
	pub fn get_pixels(&self) -> Ref<'_, [u8]> {
		Ref::map(self.pixels.borrow(), |pixels| pixels.as_slice())
	}

	//==============================================================================
	/// CPU 픽셀을 들고 있는지. (GPU 에 올린 뒤 놓았으면 false)
	//==============================================================================
	pub fn has_pixels(&self) -> bool {
		!self.pixels.borrow().is_empty()
	}

	//==============================================================================
	/// GPU 에 올린 뒤에도 CPU 픽셀을 남길지 설정. (기본 false. 픽셀을 다시 읽는 그림이 켠다)
	//==============================================================================
	pub fn set_pixels_retained(&self, is_pixels_retained: bool) {
		self.is_pixels_retained.set(is_pixels_retained);
	}

	//==============================================================================
	/// GPU 에 올린 뒤에도 CPU 픽셀을 남기는지.
	//==============================================================================
	pub fn is_pixels_retained(&self) -> bool {
		self.is_pixels_retained.get()
	}

	//==============================================================================
	/// CPU 픽셀 놓기. (ImageTextureCache 가 GPU 에 올린 뒤 부른다. 남기게 했으면 그대로 둔다)
	//==============================================================================
	pub(crate) fn release_pixels(&self) {
		if self.is_pixels_retained.get() {
			return;
		}
		let released_pixels = std::mem::take(&mut *self.pixels.borrow_mut());
		drop(released_pixels);
	}

	//==============================================================================
	/// 놓았던 CPU 픽셀 되돌리기. (Graphic 이 텍스처에서 되읽은 것)
	//==============================================================================
	pub(crate) fn restore_pixels(&self, pixels: Vec<u8>) {
		*self.pixels.borrow_mut() = pixels;
	}
}

//==============================================================================
/// 내용으로 그림 형 고르기. (WHATWG MIME 스니핑 6.1 의 그림 표에 AVIF 를 더한 것, 모르면 None)
//==============================================================================
pub fn sniff_image_mime_type(bytes: &[u8]) -> Option<&'static str> {
	if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) || bytes.starts_with(&[0x00, 0x00, 0x02, 0x00]) {
		return Some("image/x-icon");
	}
	if bytes.starts_with(b"BM") {
		return Some("image/bmp");
	}
	if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
		return Some("image/gif");
	}
	if bytes.len() >= 14 && bytes.starts_with(b"RIFF") && &bytes[8..14] == b"WEBPVP" {
		return Some("image/webp");
	}
	if bytes.starts_with(&[0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]) {
		return Some("image/png");
	}
	if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
		return Some("image/jpeg");
	}
	if is_avif_bytes(bytes) {
		return Some("image/avif");
	}
	None
}

//==============================================================================
/// AVIF 바이트인지. (ISO BMFF 의 첫 상자가 ftyp 이고 주 상표나 호환 상표에 avif 나 avis 가 있다)
//==============================================================================
fn is_avif_bytes(bytes: &[u8]) -> bool {
	if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
		return false;
	}
	let box_size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
	let box_end = box_size.min(bytes.len());
	if box_end < 16 {
		return false;
	}
	let is_avif_brand = |brand: &[u8]| -> bool {
		brand == b"avif" || brand == b"avis"
	};
	if is_avif_brand(&bytes[8..12]) {
		return true;
	}
	bytes[16..box_end].chunks_exact(4).any(is_avif_brand)
}

//==============================================================================
/// SVG 바이트인지. (앞의 빈칸과 BOM 뒤가 '<' 로 시작하고 앞 4096 바이트 안에 "<svg" 가 있다)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn is_svg_bytes(bytes: &[u8]) -> bool {
	let head_bytes = &bytes[..bytes.len().min(4096)];
	let head_text = String::from_utf8_lossy(head_bytes);
	let trimmed_text = head_text.trim_start_matches('\u{FEFF}').trim_start();
	trimmed_text.starts_with('<') && trimmed_text.contains("<svg")
}

//==============================================================================
/// SVG 풀기. (usvg 로 읽고 그림의 크기(width, height, 없으면 viewBox)로 tiny-skia 에 그린다. 글은 시스템 글꼴로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
fn decode_svg(bytes: &[u8]) -> Result<Rc<Image>, String> {
	static SYSTEM_FONT_DATABASE: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> = std::sync::OnceLock::new();
	let mut options = resvg::usvg::Options::default();
	// 글이 있는 SVG 만 시스템 글꼴을 읽는다. (처음 한 번)
	if String::from_utf8_lossy(bytes).contains("<text") {
		options.fontdb = SYSTEM_FONT_DATABASE.get_or_init(|| {
			let mut font_database = resvg::usvg::fontdb::Database::new();
			font_database.load_system_fonts();
			std::sync::Arc::new(font_database)
		}).clone();
	}
	let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|error| format!("SVG 해석 실패: {}", error))?;
	let image_size = tree.size().to_int_size();
	let mut pixmap = resvg::tiny_skia::Pixmap::new(image_size.width(), image_size.height()).ok_or_else(|| String::from("SVG 크기가 0"))?;
	resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
	Ok(Image::create_from_premultiplied_pixels(image_size.width(), image_size.height(), pixmap.take()))
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 놓으면 비고, 남기게 하면 그대로이고, 되돌리면 다시 든다.
	//==============================================================================
	#[test]
	fn release_and_retain_pixels() {
		let image = Image::create_from_pixels(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128]);
		assert!(image.has_pixels());
		assert_eq!(image.get_pixels()[7], 128);
		image.set_pixels_retained(true);
		image.release_pixels();
		assert!(image.has_pixels());
		image.set_pixels_retained(false);
		image.release_pixels();
		assert!(!image.has_pixels());
		assert!(image.get_pixels().is_empty());
		assert_eq!(image.get_width(), 2);
		image.restore_pixels(vec![1, 2, 3, 4, 5, 6, 7, 8]);
		assert!(image.has_pixels());
		assert_eq!(image.get_pixels()[4], 5);
	}

	//==============================================================================
	/// 형식: GIF, BMP, ICO 를 풀고, 데스크탑은 SVG 를 그림 크기로 그린다. (원본은 브라우저가 푸는 형식을 모두 받는다)
	//==============================================================================
	#[test]
	fn load_gif_bmp_ico_and_svg() {
		let mut source_image = ::image::RgbaImage::new(2, 2);
		source_image.put_pixel(0, 0, ::image::Rgba([255, 0, 0, 255]));
		source_image.put_pixel(1, 1, ::image::Rgba([0, 0, 255, 255]));
		for image_format in [::image::ImageFormat::Gif, ::image::ImageFormat::Bmp, ::image::ImageFormat::Ico] {
			let mut encoded_bytes: Vec<u8> = Vec::new();
			::image::DynamicImage::ImageRgba8(source_image.clone()).write_to(&mut std::io::Cursor::new(&mut encoded_bytes), image_format).expect("encode");
			let image = Image::load_from_memory(&encoded_bytes).expect("decode");
			assert_eq!((image.get_width(), image.get_height()), (2, 2), "{:?}", image_format);
			assert_eq!(&image.get_pixels()[0..4], &[255, 0, 0, 255], "{:?}", image_format);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			let svg_text = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"4\" height=\"2\"><rect width=\"2\" height=\"2\" fill=\"#ff0000\"/></svg>";
			let svg_image = Image::load_from_memory(svg_text.as_bytes()).expect("svg");
			assert_eq!((svg_image.get_width(), svg_image.get_height()), (4, 2));
			assert_eq!(&svg_image.get_pixels()[0..4], &[255, 0, 0, 255]);
			assert_eq!(svg_image.get_pixels()[3 * 4 + 3], 0);
		}
	}

	//==============================================================================
	/// 내용으로 형 고르기(WHATWG 그림 표와 AVIF), 형을 아는 바이트는 image/svg+xml 일 때만 SVG 다.
	//==============================================================================
	#[test]
	fn sniff_and_load_with_mime_type() {
		assert_eq!(sniff_image_mime_type(&[0x00, 0x00, 0x01, 0x00, 1]), Some("image/x-icon"));
		assert_eq!(sniff_image_mime_type(&[0x00, 0x00, 0x02, 0x00, 1]), Some("image/x-icon"));
		assert_eq!(sniff_image_mime_type(b"BM\x00\x00"), Some("image/bmp"));
		assert_eq!(sniff_image_mime_type(b"GIF89a\x01"), Some("image/gif"));
		assert_eq!(sniff_image_mime_type(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("image/webp"));
		assert_eq!(sniff_image_mime_type(&[0xff, 0xd8, 0xff, 0xe0]), Some("image/jpeg"));
		assert_eq!(sniff_image_mime_type(b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00mif1avif"), Some("image/avif"));
		assert_eq!(sniff_image_mime_type(b"\x00\x00\x00\x18ftypmif1\x00\x00\x00\x00mif1heic"), None);
		assert_eq!(sniff_image_mime_type(b"<svg xmlns='http://www.w3.org/2000/svg'/>"), None);
		let mut png_bytes: Vec<u8> = Vec::new();
		::image::DynamicImage::ImageRgba8(::image::RgbaImage::new(1, 1)).write_to(&mut std::io::Cursor::new(&mut png_bytes), ::image::ImageFormat::Png).expect("png");
		assert_eq!(sniff_image_mime_type(&png_bytes), Some("image/png"));
		// 형이 틀려도 래스터는 내용으로 푼다. (브라우저와 같다)
		assert!(Image::load_from_memory_with_mime_type(&png_bytes, "text/plain").is_ok());
		#[cfg(not(target_arch = "wasm32"))]
		{
			let svg_bytes = b"<svg xmlns='http://www.w3.org/2000/svg' width='3' height='1'/>";
			assert_eq!(Image::load_from_memory_with_mime_type(svg_bytes, "Image/SVG+XML; charset=utf-8").expect("svg").get_width(), 3);
			assert!(Image::load_from_memory_with_mime_type(svg_bytes, "image/png").is_err());
		}
	}
}
