//==============================================================================
//! 그림자를 깐 그림. (vanilla.js core/component/shadowimage.js 의 ShadowImage)
//! - 그림(또는 그림 안의 조각)을 노드 안에 비를 지켜 가장 크게, 가운데로 그린다.
//! - ShadowText 와 같이 그림의 모양(불투명도) 밑에 그림자를 깐 모습을 한 장으로 구워 그 한 장을 그린다.
//!   노드의 불투명도가 그림과 그림자에 한 장째로 먹는다.
//! - 그림 색을 주면 그림을 그 색 한 빛의 모양으로 칠한다. (같은 자리 위에 원래 그림을 얹어 그림자만 남기는 감쌈 등)
//! - 굽는 배율, 그림, 색, 그림자, 노드 크기가 바뀌면 다시 굽는다. 데스크탑의 줄이기는 겹친 넓이로 고르게 섞는다.
//!   같은 그림(Image 의 인스턴스 식별자와 조각)과 모양을 구운 것이 있으면 나눠 쓴다. (sharedbakecache.rs, 원본은 그림마다 번호를 붙여 열쇠로 쓴다)
//! - 그림 경로(image_path)는 Sprite 의 것과 같이 UI 문서(UIDocument)가 그림 대신 적고 읽는 값이다. 그리기에는 쓰지 않는다.
//! - 다시 구울 때 그림의 CPU 픽셀을 읽으므로 받은 그림은 GPU 에 올린 뒤에도 픽셀을 남기게 한다. 이미 놓였으면 텍스처에서 되읽는다.
//! - 웹은 원본처럼 캔버스의 drawImage(imageSmoothingQuality high)로 줄이고 색은 source-in 으로 칠한 뒤 캔버스 그림자(shadowBlur)를 깔아 굽는다. (platform/web/systemfont.rs 의 bake_shadow_image)
//!   데스크탑은 겹친 넓이로 줄이고 상자 흐림 세 번으로 그림자를 셈한다.
//! - 원본 getBakeCanvas 는 구운 캔버스 대신 구운 Image 를 돌려주는 get_bake_image 다.
//==============================================================================
use std::rc::Rc;

use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::shadowtext::compose_shadow_image;
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::node::worldnode::NodeTransform;
use crate::core::sharedbakecache;

/// 구운 그림의 가장자리 여유. (안티앨리어싱, 픽셀)
const BAKE_PADDING: i32 = 2;

//==============================================================================
/// 그림자를 깐 그림 컴포넌트. (그림 밑에 그 모양의 그림자를 깐 모습을 한 장으로 구워 그린다)
/// - 그림(또는 그림 안의 조각)을 노드 content_size 안에 비를 지켜 가장 크게, 가운데로 놓는다.
/// - 노드의 불투명도가 그림과 그림자에 한 장째로 먹는다. 그림 색을 주면 그림을 그 색 한 빛의 모양으로 칠한다.
/// - 처음은 그림 없음, 그림 색 없음, 검은 그림자, 내림 0, 번짐 0.
///
/// # 예제
/// 아이콘 밑에 흐린 그림자 깔기
/// ```rust
/// use vanilla::{Color, Image, NodeTree, ShadowImage, Vector2};
///
/// let icon = Image::create_from_pixels(32, 32, vec![255u8; 32 * 32 * 4]);
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let icon_id = node_tree.create_child(root_id, "Icon");
/// node_tree[icon_id].set_content_size(Vector2::create(64.0, 64.0));
/// let shadow_image = node_tree.add_component(icon_id, ShadowImage::new()).unwrap();
/// shadow_image.set_image(Some(icon.clone()), None);
/// shadow_image.set_shadow(Color::black(), 0.5, 2.0, 4.0);
///
/// let shadow_image = node_tree[icon_id].get_component::<ShadowImage>().unwrap();
/// assert!(shadow_image.get_image().is_some());
/// assert!(icon.is_pixels_retained());
/// assert!(shadow_image.get_bake_image().is_none());
/// ```
//==============================================================================
pub struct ShadowImage {
	component_base: ComponentBase,
	image: Option<Rc<Image>>,
	image_rect: Option<Rect>,
	image_color: Option<Color>,
	shadow_color: Color,
	shadow_offset: f64,
	shadow_blur: f64,
	image_version: u32,
	bake_image: Option<Rc<Image>>,
	bake_scale: f64,
	bake_padding: f64,
	bake_key: String,
	image_path: String,
}

//==============================================================================
/// 그림의 조각을 그릴 크기로 바꾼 곧은 알파 RGBA. (줄일 때는 겹친 넓이로 고르게, 키울 때는 가까운 픽셀)
//==============================================================================
fn resample_image(image: &Image, source_rect: [f64; 4], target_width: i32, target_height: i32) -> Vec<u8> {
	let source_pixels = image.get_pixels();
	let source_width = image.get_width() as i32;
	let source_height = image.get_height() as i32;
	let mut pixels = vec![0u8; (target_width * target_height * 4) as usize];
	let step_x = source_rect[2] / target_width as f64;
	let step_y = source_rect[3] / target_height as f64;
	for target_y in 0..target_height {
		let start_y = source_rect[1] + target_y as f64 * step_y;
		for target_x in 0..target_width {
			let start_x = source_rect[0] + target_x as f64 * step_x;
			let mut sum = [0.0f64; 4];
			let mut weight_sum = 0.0f64;
			let sample_start_y = start_y.floor() as i32;
			let sample_end_y = (start_y + step_y.max(1.0)).ceil() as i32;
			let sample_start_x = start_x.floor() as i32;
			let sample_end_x = (start_x + step_x.max(1.0)).ceil() as i32;
			for sample_y in sample_start_y..sample_end_y {
				if sample_y < 0 || sample_y >= source_height {
					continue;
				}
				let overlap_y = ((sample_y + 1) as f64).min(start_y + step_y.max(1.0)) - (sample_y as f64).max(start_y);
				for sample_x in sample_start_x..sample_end_x {
					if sample_x < 0 || sample_x >= source_width {
						continue;
					}
					let overlap_x = ((sample_x + 1) as f64).min(start_x + step_x.max(1.0)) - (sample_x as f64).max(start_x);
					let weight = overlap_x.max(0.0) * overlap_y.max(0.0);
					if weight <= 0.0 {
						continue;
					}
					let offset = ((sample_y * source_width + sample_x) * 4) as usize;
					let alpha = source_pixels[offset + 3] as f64;
					// 색은 알파를 곱해 섞어 가장자리가 검게 번지지 않게 한다.
					sum[0] += source_pixels[offset] as f64 * alpha * weight;
					sum[1] += source_pixels[offset + 1] as f64 * alpha * weight;
					sum[2] += source_pixels[offset + 2] as f64 * alpha * weight;
					sum[3] += alpha * weight;
					weight_sum += weight;
				}
			}
			let offset = ((target_y * target_width + target_x) * 4) as usize;
			if weight_sum > 0.0 && sum[3] > 0.0 {
				pixels[offset] = (sum[0] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 1] = (sum[1] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 2] = (sum[2] / sum[3]).round().clamp(0.0, 255.0) as u8;
				pixels[offset + 3] = (sum[3] / weight_sum).round().clamp(0.0, 255.0) as u8;
			}
		}
	}
	pixels
}

#[crate::reflect]
impl ShadowImage {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 그림이 없고 검은 그림자(내림 0, 번짐 0)인 새 컴포넌트.
	//==============================================================================
	pub fn new() -> ShadowImage {
		ShadowImage {
			component_base: ComponentBase::new(),
			image: None,
			image_rect: None,
			image_color: None,
			shadow_color: Color::black(),
			shadow_offset: 0.0,
			shadow_blur: 0.0,
			image_version: 0,
			bake_image: None,
			bake_scale: 1.0,
			bake_padding: 0.0,
			bake_key: String::new(),
			image_path: String::new(),
		}
	}

	//==============================================================================
	/// 출력.
	/// - 그림이 없거나, 조각이나 노드 크기가 0 이하면 그리지 않는다.
	/// - 굽는 배율, 그림, 조각, 색, 그림자, 노드 크기가 지난번과 다르면 다시 굽는다. 그림의 CPU 픽셀을 아직 되읽지 못했으면 이번에는 그리지 않고 다음에 다시 굽는다.
	/// - 구운 한 장은 그림자 여백만큼 넓어서 그림 자리보다 왼쪽 위로 여백만큼 당겨 그린다.
	///
	/// # 인자
	/// - `transform`: 그릴 노드 트랜스폼. (content_size 를 읽는다)
	/// - `graphic`: 그릴 그래픽. (노드 트랜스폼이 걸린 상태, 굽는 배율과 텍스처 되읽기에 쓴다)
	//==============================================================================
	pub fn draw_shadow_image(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		let image = match &self.image {
			Some(image) => {
				image.clone()
			}
			None => {
				return;
			}
		};
		let source_rect = match self.image_rect {
			Some(image_rect) => {
				[image_rect.position.x, image_rect.position.y, image_rect.size.x, image_rect.size.y]
			}
			None => {
				[0.0, 0.0, image.get_width() as f64, image.get_height() as f64]
			}
		};
		let content_size = transform.get_content_size();
		if source_rect[2] <= 0.0 || source_rect[3] <= 0.0 || content_size.x <= 0.0 || content_size.y <= 0.0 {
			return;
		}

		// 노드 안에 그림의 비를 지켜 가장 크게, 가운데로.
		let fit_scale = (content_size.x / source_rect[2]).min(content_size.y / source_rect[3]);
		let image_width = source_rect[2] * fit_scale;
		let image_height = source_rect[3] * fit_scale;
		let bake_scale = graphic.calculate_text_bake_scale();
		let bake_key = format!("ShadowImage|{}|{:?}|{:?}|{:?}|{}|{}|{}|{}|{}", image.get_instance_id(), source_rect, self.image_color, self.shadow_color, self.shadow_offset, self.shadow_blur, image_width, image_height, bake_scale);
		if bake_key != self.bake_key {
			// 같은 모양을 구운 것이 있으면 나눠 쓰고, 없으면 굽는다.
			// 구울 때 그림이 다른 곳에서 먼저 그려져 CPU 픽셀을 놓았으면 텍스처에서 되읽는다. (못 읽으면 다음 프레임에 다시)
			let bake_image = sharedbakecache::acquire_shared_bake(&bake_key, || {
				if !graphic.restore_image_pixels(&image) {
					return None;
				}
				self.bake(&image, source_rect, image_width, image_height, bake_scale)
			});
			if bake_image.is_none() {
				return;
			}
			self.bake_image = bake_image;
			self.bake_scale = bake_scale;
			self.bake_padding = ShadowImage::calculate_bake_padding(self.shadow_blur, bake_scale) as f64 / bake_scale;
			self.bake_key = bake_key;
		}
		let bake_image = match &self.bake_image {
			Some(bake_image) => {
				bake_image.clone()
			}
			None => {
				return;
			}
		};
		let quad_x = (content_size.x - image_width) * 0.5 - self.bake_padding;
		let quad_y = (content_size.y - image_height) * 0.5 - self.bake_padding;
		let bake_width = bake_image.get_width() as f64;
		let bake_height = bake_image.get_height() as f64;
		graphic.draw_image_with_source_and_destination(&bake_image, 0.0, 0.0, bake_width, bake_height, quad_x, quad_y, bake_width / self.bake_scale, bake_height / self.bake_scale);
	}

	//==============================================================================
	/// 굽기. (그릴 크기로 바꾼 그림을 가장자리를 둘러 놓고, 그 모양으로 그림자를 깐다)
	//==============================================================================
	fn bake(&self, image: &Image, source_rect: [f64; 4], image_width: f64, image_height: f64, bake_scale: f64) -> Option<Rc<Image>> {
		let scaled_width = ((image_width * bake_scale).round() as i32).max(1);
		let scaled_height = ((image_height * bake_scale).round() as i32).max(1);
		// 웹: 원본처럼 캔버스의 drawImage(imageSmoothingQuality high)로 줄이고 캔버스 그림자를 깐다.
		if cfg!(target_arch = "wasm32") {
			#[cfg(target_arch = "wasm32")]
			{
				let browser_bake = crate::platform::web::systemfont::bake_shadow_image(&image.get_pixels(), image.get_width(), image.get_height(), source_rect, scaled_width as u32, scaled_height as u32, self.image_color, self.shadow_color, ((self.shadow_offset * bake_scale) as f32) as f64, ((self.shadow_blur * bake_scale) as f32) as f64)?;
				return Some(Image::create_from_pixels(browser_bake.width, browser_bake.height, browser_bake.pixels));
			}
		}
		let scaled_blur = self.shadow_blur * bake_scale;
		let scaled_offset = (self.shadow_offset * bake_scale).round() as i32;
		let padding = ShadowImage::calculate_bake_padding(self.shadow_blur, bake_scale);
		let bake_width = scaled_width + padding * 2;
		let bake_height = scaled_height + padding * 2 + scaled_offset.max(0);
		let scaled_pixels = resample_image(image, source_rect, scaled_width, scaled_height);
		let mut shape_pixels = vec![0u8; (bake_width * bake_height * 4) as usize];
		let mut shape_coverage = vec![0u8; (bake_width * bake_height) as usize];
		for y in 0..scaled_height {
			for x in 0..scaled_width {
				let source_offset = ((y * scaled_width + x) * 4) as usize;
				let target_index = ((y + padding) * bake_width + x + padding) as usize;
				let target_offset = target_index * 4;
				let alpha = scaled_pixels[source_offset + 3];
				match self.image_color {
					Some(image_color) => {
						shape_pixels[target_offset] = (image_color.red * 255.0).round() as u8;
						shape_pixels[target_offset + 1] = (image_color.green * 255.0).round() as u8;
						shape_pixels[target_offset + 2] = (image_color.blue * 255.0).round() as u8;
						shape_pixels[target_offset + 3] = (alpha as f64 * image_color.alpha).round() as u8;
					}
					None => {
						shape_pixels[target_offset..target_offset + 4].copy_from_slice(&scaled_pixels[source_offset..source_offset + 4]);
					}
				}
				shape_coverage[target_index] = alpha;
			}
		}
		Some(compose_shadow_image(&shape_coverage, Some(&shape_pixels), bake_width, bake_height, Color::white(), self.shadow_color, scaled_blur, scaled_offset))
	}

	//==============================================================================
	/// 구운 그림의 가장자리 여유. (픽셀, 번지는 그림자가 잘리지 않게 번지는 만큼 넓힌다)
	//==============================================================================
	fn calculate_bake_padding(shadow_blur: f64, bake_scale: f64) -> i32 {
		let scaled_blur = shadow_blur * bake_scale;
		if cfg!(target_arch = "wasm32") {
			// 웹: 원본 bakeImage 와 같다. (캔버스 그림자의 번짐)
			BAKE_PADDING + scaled_blur.ceil() as i32
		}
		else {
			BAKE_PADDING + scaled_blur.ceil() as i32 * 2
		}
	}

	//==============================================================================
	/// 구운 그림이 그림자 내림으로 아래로 늘어나는 만큼. (픽셀, 웹은 원본처럼 올림, 데스크탑은 반올림한 내림만큼 옮겨 셈한다)
	//==============================================================================
	fn calculate_bake_offset(shadow_offset: f64, bake_scale: f64) -> i32 {
		let scaled_offset = (shadow_offset * bake_scale).max(0.0);
		if cfg!(target_arch = "wasm32") {
			scaled_offset.ceil() as i32
		}
		else {
			scaled_offset.round() as i32
		}
	}

	//==============================================================================
	/// 그림 설정. (조각을 주지 않으면 그림 전체)
	/// - 받은 그림은 다시 구울 때 픽셀을 읽으므로 GPU 에 올린 뒤에도 CPU 픽셀을 남기게 한다. (Image::set_pixels_retained(true))
	///
	/// # 인자
	/// - `image`: 그릴 그림. (None 이면 그리지 않는다)
	/// - `image_rect`: 그림 안의 조각. (픽셀, 그림 왼쪽 위 기준, None 이면 그림 전체)
	///
	/// # 예제
	/// 시트의 오른쪽 칸만 그리기
	/// ```rust
	/// use vanilla::{Image, Rect, ShadowImage};
	///
	/// let sheet = Image::create_from_pixels(64, 32, vec![255u8; 64 * 32 * 4]);
	/// let mut shadow_image = ShadowImage::new();
	/// shadow_image.set_image(Some(sheet), Some(Rect::create(32.0, 0.0, 32.0, 32.0)));
	/// assert_eq!(shadow_image.get_image_rect(), Some(Rect::create(32.0, 0.0, 32.0, 32.0)));
	///
	/// shadow_image.set_image(None, None);
	/// assert!(shadow_image.get_image().is_none());
	/// assert_eq!(shadow_image.get_image_rect(), None);
	/// ```
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>, image_rect: Option<Rect>) {
		if let Some(image) = &image {
			image.set_pixels_retained(true);
		}
		self.image = image;
		self.image_rect = image_rect;
		self.image_version += 1;
	}

	//==============================================================================
	/// 그림 반환.
	///
	/// # 돌려줌
	/// 그리는 그림. 정하지 않았으면 None.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.image.as_ref()
	}

	//==============================================================================
	/// 그림 안의 조각 반환. (None 이면 그림 전체)
	///
	/// # 돌려줌
	/// 그림 안의 조각. (픽셀, None 이면 그림 전체)
	//==============================================================================
	pub fn get_image_rect(&self) -> Option<Rect> {
		self.image_rect
	}

	//==============================================================================
	/// 그림을 한 빛으로 칠할 색 설정. (None 이면 그림 그대로)
	///
	/// # 인자
	/// - `image_color`: 그림 모양을 칠할 색. (알파는 그림 알파에 곱한다, None 이면 그림 색 그대로, 처음 None)
	//==============================================================================
	pub fn set_image_color(&mut self, image_color: Option<Color>) {
		self.image_color = image_color;
	}

	//==============================================================================
	/// 그림을 한 빛으로 칠할 색 반환.
	///
	/// # 돌려줌
	/// 그림 모양을 칠할 색. 그림 색 그대로면 None.
	//==============================================================================
	pub fn get_image_color(&self) -> Option<Color> {
		self.image_color
	}

	//==============================================================================
	/// 그림 경로 설정. (UI 문서가 적고 읽는다)
	///
	/// # 인자
	/// - `image_path`: 그림 경로. (적어 두기만 하고 그림을 읽지 않는다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	/// 그림 경로 반환. (정하지 않았으면 빈 글. UI 문서는 Sprite 처럼 imagePath 로 따로 적으므로 반사 저장 속성에서 뺀다)
	///
	/// # 돌려줌
	/// 그림 경로. (정하지 않았으면 빈 글)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}

	//==============================================================================
	/// 그림자 설정. (색, 불투명도, 아래로 내려가는 만큼, 번지는 만큼, 원본 setShadow(색, 불투명도, 내려가는 만큼, 번지는 만큼) 과 같다)
	///
	/// # 인자
	/// - `shadow_color`: 그림자 색. (Color 또는 "#rrggbb" 같은 문자열, 색의 알파는 읽지 않는다, 처음 검정)
	/// - `shadow_opacity`: 그림자 불투명도. (0 ~ 1, 처음 1)
	/// - `shadow_offset`: 그림자가 아래로 내려가는 만큼. (노드 좌표, 처음 0)
	/// - `shadow_blur`: 그림자가 번지는 만큼. (노드 좌표, 0 보다 작으면 0, 처음 0)
	//==============================================================================
	pub fn set_shadow(&mut self, shadow_color: impl Into<Color>, shadow_opacity: f64, shadow_offset: f64, shadow_blur: f64) {
		let mut shadow_color: Color = shadow_color.into();
		shadow_color.alpha = shadow_opacity.clamp(0.0, 1.0);
		self.shadow_color = shadow_color;
		self.shadow_offset = shadow_offset;
		self.shadow_blur = shadow_blur.max(0.0);
	}

	//==============================================================================
	/// 구운 그림 반환. (아직 굽지 않았으면 None, 원본 getBakeCanvas)
	///
	/// # 돌려줌
	/// 마지막으로 구운 그림. (그림자 여백을 둘러 굽는 배율로 구운 한 장) 아직 굽지 않았으면 None.
	//==============================================================================
	#[reflect(name = "BakeCanvas")]
	pub fn get_bake_image(&self) -> Option<&Rc<Image>> {
		self.bake_image.as_ref()
	}
}

impl Default for ShadowImage {
	//==============================================================================
	/// 기본값. (new 와 같다)
	//==============================================================================
	fn default() -> ShadowImage {
		ShadowImage::new()
	}
}

#[crate::reflect]
impl Component for ShadowImage {
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
	/// 컴포넌트 이름. ("ShadowImage")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"ShadowImage"
	}

	//==============================================================================
	/// tick 이 할 일이 있는지. (그리기만 하므로 false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그리는지. (true, 그림자 여백은 calculate_drawn_rect 가 더한다)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 화면 밖 거르기에 쓰는, 실제로 그리는 사각형. (content 영역에 그림자 여백(번짐, 아래로 내림)을 더한다, content_size 가 0 이하면 None)
	//==============================================================================
	fn calculate_drawn_rect(&mut self, transform: &NodeTransform, graphic: &mut Graphic) -> Option<Rect> {
		// 그림은 content 안에 들지만 구운 한 장은 그림자 여백(번짐, 아래로 내림)만큼 넓다. (bake 와 같은 셈)
		let content_size = transform.get_content_size();
		if content_size.x <= 0.0 || content_size.y <= 0.0 {
			return None;
		}
		let bake_scale = graphic.calculate_text_bake_scale();
		let padding = ShadowImage::calculate_bake_padding(self.shadow_blur, bake_scale) as f64 / bake_scale;
		let offset = ShadowImage::calculate_bake_offset(self.shadow_offset, bake_scale) as f64 / bake_scale;
		Some(Rect::create(-padding, -padding, content_size.x + padding * 2.0, content_size.y + padding * 2.0 + offset))
	}

	//==============================================================================
	/// 출력. (draw_shadow_image)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.draw_shadow_image(transform, graphic);
	}
}
