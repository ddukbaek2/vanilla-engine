//==============================================================================
//! 머티리얼. (원본 experimental/graphics/material.js)
//! - 셰이더 템플릿 참조 + metallic-roughness PBR 파라미터/텍스처, 포맷 중립. 모델은 머티리얼 인스턴스를 적용만 한다.
//! - 임포터가 만든 중립 머티리얼 서술(팩터 + 이미지 바이트, MaterialDescription)로부터 GL 텍스처를 만든다.
//! - FBX 의 스펙큘러/글로스 맵은 glossiness 경로로 러프니스를 근사한다.
//! - 텍스처 유닛 배치는 원본과 같다: 0 = base_color, 1 = 섀도우 맵 전용(사용 금지), 2 ~ 8 = normal / metallicRoughness /
//!   glossiness / occlusion / emissive / specular / opacity, 추가 슬롯은 9 이상.
//! - 원본과 다른 것:
//!   - 셰이더 프로그램은 `Rc<ShaderProgram>` 으로 렌더러와 나눠 든다. 모델의 드로어블은 `Rc<Material>` 로 가리킨다. (원본 참조 의미)
//!   ```text
//!     원본이 인스턴스를 고치는 set_texture / set_use_alpha_cutout / set_subsurface_factor 는 &self 로 부를 수 있게 안에서 Cell 로 든다.
//!   ```
//!   - 이미지 해석은 원본 createImageBitmap(비동기) 대신 image 크레이트(png, jpeg, webp, gif, bmp, ico)로 그 자리에서 한다. 그래서 async 가 없다.
//!   ```text
//!     형은 원본 크롬처럼 해석에 쓰지 않고 내용으로 가린다. (크롬 createImageBitmap(Blob) 도 Blob 의 형이 아니라 내용으로 해독기를 고른다)
//!     원본처럼 flip_y 면 위아래를 뒤집고, is_color_data 면 SRGB8_ALPHA8 로 올린다.
//!   ```
//!   - 원본 loadImageDescription(url) 의 Promise 는 ImageAsset 처럼 받기 손잡이(ImageDescriptionLoadHandle)다. load_image_description(엔진, 애셋 경로, 옵션)이
//!   ```text
//!     엔진의 애셋 받기(데스크탑은 애셋 폴더 후보, 웹은 asset_base_url 에서 fetch)로 바이트를 부탁하고, 다 받으면 get_image_description 이 서술을 만든다.
//!     손잡이는 그대로 await 할 수 있다. (엔진 작업 안에서 Material::load_image_description(engine, 경로, 옵션).await 가 원본 await 처럼 서술을 낸다)
//!     mime_type 은 원본처럼 응답의 content-type 이다. (웹 fetch 응답 머리, 머리가 없으면 None) 데스크탑에는 응답이 없어 내용으로 고른 형이다.
//!     (Image::sniff_image_mime_type, WHATWG MIME 스니핑의 그림 표와 AVIF, 모르면 None) 바이트를 받는 create_image_description(bytes) 는 원본과 같다.
//!   ```
//!   - 원본 이미지 서술의 source(캔버스/비트맵)는 RGBA 픽셀(ImageSource)로 받는다.
//==============================================================================
use crate::core::asset::AssetLoadHandle;
use crate::core::engine::Engine;
use crate::core::graphic::GlTexture;
use crate::core::shaderprogram::ShaderProgram;
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

//==============================================================================
/// 전역 상수 목록.
//==============================================================================
/// 텍스처 유닛 배치. (1번은 SkinnedModel 의 섀도우 맵 전용, 사용 금지)
pub const TEXTURE_UNIT_BASECOLOR: u32 = 0;
pub const TEXTURE_UNIT_SHADOWMAP: u32 = 1;
pub const TEXTURE_UNIT_NORMAL: u32 = 2;
pub const TEXTURE_UNIT_METALLICROUGHNESS: u32 = 3;
pub const TEXTURE_UNIT_GLOSSINESS: u32 = 4;
pub const TEXTURE_UNIT_OCCLUSION: u32 = 5;
pub const TEXTURE_UNIT_EMISSIVE: u32 = 6;
pub const TEXTURE_UNIT_SPECULAR: u32 = 7;
pub const TEXTURE_UNIT_OPACITY: u32 = 8;

//==============================================================================
/// 이미 푼 RGBA 픽셀. (원본 이미지 서술의 source, 캔버스/비트맵 자리, 스트레이트 알파, 위에서 아래로)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ImageSource {
	pub width: u32,
	pub height: u32,
	pub pixels: Vec<u8>,
}

//==============================================================================
/// 이미지 서술. (원본 { bytes, mime_type, flip_y, is_color_data, anisotropy } 또는 { source, flip_y })
/// - bytes 는 png / jpeg / webp 파일 내용이다. 같은 이미지를 여러 텍스처가 쓰면 Rc 로 나눠 든다. (FBX 비디오 중복 참조)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ImageDescription {
	pub bytes: Rc<Vec<u8>>,
	pub mime_type: Option<String>,
	pub source: Option<Rc<ImageSource>>,
	pub flip_y: bool,
	pub is_color_data: bool,
	pub anisotropy: f32,
}

//==============================================================================
/// 이미지 서술 옵션. (원본 load_image_description 의 options { flip_y, is_color_data, anisotropy })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageDescriptionOptions {
	pub flip_y: bool,
	pub is_color_data: bool,
	pub anisotropy: f32,
}

//==============================================================================
/// 이미지 서술 받기 손잡이. (Material::load_image_description 이 돌려준다. 원본 loadImageDescription 의 Promise 자리)
/// - 씬은 get_load_handle 을 SceneBase::track_load_handle 로 맡겨 다 받을 때까지 initialize 를 미룰 수 있다.
//==============================================================================
#[derive(Clone, Debug)]
pub struct ImageDescriptionLoadHandle {
	load_handle: AssetLoadHandle,
	options: Option<ImageDescriptionOptions>,
}

impl ImageDescriptionLoadHandle {
	//==============================================================================
	/// 바이트 읽기 손잡이. (씬에 맡길 때)
	//==============================================================================
	pub fn get_load_handle(&self) -> &AssetLoadHandle {
		&self.load_handle
	}

	//==============================================================================
	/// 끝났는지. (받았거나 실패했거나)
	//==============================================================================
	pub fn is_finished(&self) -> bool {
		self.load_handle.is_finished()
	}

	//==============================================================================
	/// 받은 바이트로 이미지 서술 만들기. (받는 중이면 None, 받기에 실패하면 Err. 원본 문구 "Material image load failed: url")
	//==============================================================================
	pub fn get_image_description(&self) -> Option<Result<ImageDescription, String>> {
		if !self.load_handle.is_finished() {
			return None;
		}
		let asset_path = self.load_handle.get_asset_path();
		match self.load_handle.get_bytes() {
			Some(bytes) => {
				let mime_type = Material::read_response_mime_type(&self.load_handle, &bytes);
				Some(Ok(Material::create_image_description(bytes.as_ref().clone(), mime_type, self.options)))
			}
			None => {
				Some(Err(format!("Material image load failed: {}", asset_path)))
			}
		}
	}
}

//==============================================================================
/// 이미지 서술 받기를 기다리는 퓨처. (ImageDescriptionLoadHandle 을 await 하면 된다. 원본 loadImageDescription 의 Promise 가 풀린 값)
//==============================================================================
#[derive(Clone, Debug)]
pub struct ImageDescriptionLoadFuture {
	image_description_load_handle: ImageDescriptionLoadHandle,
}

impl Future for ImageDescriptionLoadFuture {
	type Output = Result<ImageDescription, String>;

	//==============================================================================
	/// 끝났는지 보기. (아직이면 다음 프레임에 다시 본다)
	//==============================================================================
	fn poll(self: Pin<&mut ImageDescriptionLoadFuture>, context: &mut Context<'_>) -> Poll<Result<ImageDescription, String>> {
		match self.image_description_load_handle.get_image_description() {
			Some(image_description_result) => {
				Poll::Ready(image_description_result)
			}
			None => {
				context.waker().wake_by_ref();
				Poll::Pending
			}
		}
	}
}

impl IntoFuture for ImageDescriptionLoadHandle {
	type Output = Result<ImageDescription, String>;
	type IntoFuture = ImageDescriptionLoadFuture;

	//==============================================================================
	/// 손잡이를 그대로 await 하기.
	//==============================================================================
	fn into_future(self) -> ImageDescriptionLoadFuture {
		ImageDescriptionLoadFuture {
			image_description_load_handle: self,
		}
	}
}

//==============================================================================
/// 중립 머티리얼 서술. (원본 create_default_description 이 만드는 객체, 팩터 + 이미지, GL 무관)
//==============================================================================
#[derive(Clone, Debug)]
pub struct MaterialDescription {
	pub base_color_factor: [f32; 3],
	pub metallic_factor: f32,
	pub roughness_factor: f32,
	pub emissive_factor: [f32; 3],
	pub base_color_image: Option<ImageDescription>,
	pub normal_image: Option<ImageDescription>,
	pub metallic_roughness_image: Option<ImageDescription>,
	pub glossiness_image: Option<ImageDescription>,
	pub occlusion_image: Option<ImageDescription>,
	pub emissive_image: Option<ImageDescription>,
	pub specular_image: Option<ImageDescription>,
	pub opacity_image: Option<ImageDescription>,
}

//==============================================================================
/// 머티리얼.
//==============================================================================
pub struct Material {
	shader_program: Rc<ShaderProgram>,
	base_color_factor: [f32; 3],
	metallic_factor: f32,
	roughness_factor: f32,
	emissive_factor: [f32; 3],
	use_glossiness: bool,
	use_alpha_cutout: Cell<bool>,
	base_color_texture: Option<GlTexture>,
	normal_texture: Option<GlTexture>,
	metallic_roughness_texture: Option<GlTexture>,
	glossiness_texture: Option<GlTexture>,
	occlusion_texture: Option<GlTexture>,
	emissive_texture: Option<GlTexture>,
	specular_texture: Option<GlTexture>,
	opacity_texture: Option<GlTexture>,
	extra_texture_bindings: RefCell<Vec<(String, u32, Option<GlTexture>)>>,
	subsurface_factor: Cell<f32>,
}

impl Material {
	//==============================================================================
	/// 생성. (텍스처 없음, 흰색 팩터, 메탈릭 1, 러프니스 1)
	//==============================================================================
	pub fn new(shader_program: Rc<ShaderProgram>) -> Material {
		Material {
			shader_program,
			base_color_factor: [1.0, 1.0, 1.0],
			metallic_factor: 1.0,
			roughness_factor: 1.0,
			emissive_factor: [0.0, 0.0, 0.0],
			use_glossiness: false,
			use_alpha_cutout: Cell::new(false),
			base_color_texture: None,
			normal_texture: None,
			metallic_roughness_texture: None,
			glossiness_texture: None,
			occlusion_texture: None,
			emissive_texture: None,
			specular_texture: None,
			opacity_texture: None,
			extra_texture_bindings: RefCell::new(Vec::new()),
			subsurface_factor: Cell::new(1.0),
		}
	}

	//==============================================================================
	/// 기본 머티리얼 서술 생성. (흰색 무광 비금속)
	//==============================================================================
	pub fn create_default_description() -> MaterialDescription {
		MaterialDescription {
			base_color_factor: [1.0, 1.0, 1.0],
			metallic_factor: 0.0,
			roughness_factor: 1.0,
			emissive_factor: [0.0, 0.0, 0.0],
			base_color_image: None,
			normal_image: None,
			metallic_roughness_image: None,
			glossiness_image: None,
			occlusion_image: None,
			emissive_image: None,
			specular_image: None,
			opacity_image: None,
		}
	}

	//==============================================================================
	/// 중립 머티리얼 서술로부터 생성. (이미지 바이트 → GL 텍스처, 못 풀면 오류)
	/// - 중간에 실패하면 그때까지 지은 텍스처를 지우고 오류를 돌려준다.
	//==============================================================================
	pub fn create_from_description(gl: &glow::Context, shader_program: Rc<ShaderProgram>, description: &MaterialDescription) -> Result<Material, String> {
		let mut material = Material::new(shader_program);
		material.base_color_factor = description.base_color_factor;
		material.metallic_factor = description.metallic_factor;
		material.roughness_factor = description.roughness_factor;
		material.emissive_factor = description.emissive_factor;
		material.use_glossiness = description.glossiness_image.is_some();
		material.use_alpha_cutout.set(description.opacity_image.is_some());
		if let Err(error) = material.create_description_textures(gl, description) {
			material.destroy(gl);
			return Err(error);
		}
		Ok(material)
	}

	//==============================================================================
	/// 서술의 이미지 여덟 칸을 기본 슬롯 텍스처로 짓는다.
	///
	/// # 돌려줌
	/// 하나라도 못 지으면 그 오류. (그때까지 지은 텍스처는 슬롯에 남아 있다, 부른 쪽이 destroy 로 지운다)
	//==============================================================================
	fn create_description_textures(&mut self, gl: &glow::Context, description: &MaterialDescription) -> Result<(), String> {
		self.base_color_texture = Some(Material::create_texture(gl, description.base_color_image.as_ref(), [255, 255, 255, 255])?);
		self.normal_texture = Some(Material::create_texture(gl, description.normal_image.as_ref(), [128, 128, 255, 255])?);
		self.metallic_roughness_texture = Some(Material::create_texture(gl, description.metallic_roughness_image.as_ref(), [255, 255, 255, 255])?);
		self.glossiness_texture = Some(Material::create_texture(gl, description.glossiness_image.as_ref(), [255, 255, 255, 255])?);
		self.occlusion_texture = Some(Material::create_texture(gl, description.occlusion_image.as_ref(), [255, 255, 255, 255])?);
		self.emissive_texture = Some(Material::create_texture(gl, description.emissive_image.as_ref(), [255, 255, 255, 255])?);
		self.specular_texture = Some(Material::create_texture(gl, description.specular_image.as_ref(), [255, 255, 255, 255])?);
		self.opacity_texture = Some(Material::create_texture(gl, description.opacity_image.as_ref(), [255, 255, 255, 255])?);
		Ok(())
	}

	//==============================================================================
	/// 텍스처 생성. (이미지 서술 → GL 텍스처, 없으면 1x1 단색 대체)
	/// - is_color_data: sRGB 로 저장된 색 텍스처, 하드웨어 디코드(SRGB8_ALPHA8)로 선형 공간 샘플링.
	/// - anisotropy: 이방성 필터 단계. (확장 미지원 시 무시)
	//==============================================================================
	pub fn create_texture(gl: &glow::Context, image_description: Option<&ImageDescription>, fallback_color: [u8; 4]) -> Result<GlTexture, String> {
		let has_bytes = image_description.map(|description| !description.bytes.is_empty()).unwrap_or(false);
		let has_source = image_description.map(|description| description.source.is_some()).unwrap_or(false);
		let image_description = match image_description {
			Some(image_description) if has_bytes || has_source => {
				image_description
			}
			_ => {
				return unsafe {
					let texture = gl.create_texture()?;
					gl.bind_texture(glow::TEXTURE_2D, Some(texture));
					gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&fallback_color)));
					Ok(texture)
				};
			}
		};

		// 픽셀 준비. (원본 createImageBitmap, flip_y 면 위아래를 뒤집는다)
		let (width, height, mut pixels) = match &image_description.source {
			Some(image_source) => {
				(image_source.width, image_source.height, image_source.pixels.clone())
			}
			None => {
				decode_image_bytes(&image_description.bytes)?
			}
		};
		if pixels.len() < (width as usize) * (height as usize) * 4 {
			return Err(format!("Material: image pixel count mismatch ({}x{})", width, height));
		}
		if image_description.flip_y {
			flip_pixel_rows(&mut pixels, width as usize, height as usize);
		}
		let internal_format = if image_description.is_color_data {
			glow::SRGB8_ALPHA8
		}
		else {
			glow::RGBA8
		};
		unsafe {
			let texture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
			gl.tex_image_2d(glow::TEXTURE_2D, 0, internal_format as i32, width as i32, height as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&pixels)));
			gl.generate_mipmap(glow::TEXTURE_2D);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
			if image_description.anisotropy > 0.0 {
				let has_anisotropic_extension = gl.supported_extensions().iter().any(|extension_name| extension_name.contains("texture_filter_anisotropic"));
				if has_anisotropic_extension {
					let maximum_anisotropy = gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT);
					let anisotropy = image_description.anisotropy.min(maximum_anisotropy);
					gl.tex_parameter_f32(glow::TEXTURE_2D, glow::TEXTURE_MAX_ANISOTROPY_EXT, anisotropy);
				}
			}
			Ok(texture)
		}
	}

	//==============================================================================
	/// 바이트로 이미지 서술 생성. (원본 load_image_description 의 fetch 뒤 부분, 웹은 BlobAsset 바이트를 넘긴다)
	//==============================================================================
	pub fn create_image_description(bytes: Vec<u8>, mime_type: Option<String>, options: Option<ImageDescriptionOptions>) -> ImageDescription {
		let resolved_options = options.unwrap_or_default();
		ImageDescription {
			bytes: Rc::new(bytes),
			mime_type,
			source: None,
			flip_y: resolved_options.flip_y,
			is_color_data: resolved_options.is_color_data,
			anisotropy: resolved_options.anisotropy,
		}
	}

	//==============================================================================
	/// 애셋 경로(웹은 URL)로부터 이미지 서술 받기 부탁. (원본 정적 loadImageDescription, 다 받으면 손잡이의 get_image_description 이 만든다)
	//==============================================================================
	pub fn load_image_description(engine: &Engine, url: &str, options: Option<ImageDescriptionOptions>) -> ImageDescriptionLoadHandle {
		ImageDescriptionLoadHandle {
			load_handle: engine.request_asset_bytes(url),
			options,
		}
	}

	//==============================================================================
	/// 받은 이미지의 mime_type. (원본 response.headers.get("content-type"), 웹은 응답 머리, 데스크탑은 응답이 없어 내용으로 고른 형)
	//==============================================================================
	fn read_response_mime_type(load_handle: &AssetLoadHandle, bytes: &[u8]) -> Option<String> {
		if let Some(content_type) = load_handle.get_content_type() {
			return Some(content_type);
		}
		#[cfg(not(target_arch = "wasm32"))]
		{
			crate::core::image::sniff_image_mime_type(bytes).map(String::from)
		}
		#[cfg(target_arch = "wasm32")]
		{
			let _ = bytes;
			None
		}
	}

	//==============================================================================
	/// 추가 텍스처 설정. (셰이더 템플릿 고유 슬롯, 기본 슬롯 밖의 유니폼 이름 / 텍스처 유닛)
	/// - 유닛 1 은 섀도우 맵 전용, 0 / 2~8 은 기본 슬롯이므로 9 이상을 사용한다. 같은 이름은 자리를 지키고 값만 바꾼다.
	//==============================================================================
	pub fn set_texture(&self, uniform_name: &str, texture_unit: u32, texture: Option<GlTexture>) {
		let mut extra_texture_bindings = self.extra_texture_bindings.borrow_mut();
		for binding in extra_texture_bindings.iter_mut() {
			if binding.0 == uniform_name {
				binding.1 = texture_unit;
				binding.2 = texture;
				return;
			}
		}
		extra_texture_bindings.push((uniform_name.to_string(), texture_unit, texture));
	}

	//==============================================================================
	/// 적용. (참조 셰이더에 유니폼 설정 + 텍스처 유닛 바인드)
	//==============================================================================
	pub fn apply(&self, gl: &glow::Context) {
		let shader_program = &self.shader_program;
		let base_color_factor_location = shader_program.get_uniform_location(gl, "baseColorFactor");
		let metallic_factor_location = shader_program.get_uniform_location(gl, "metallicFactor");
		let roughness_factor_location = shader_program.get_uniform_location(gl, "roughnessFactor");
		let emissive_factor_location = shader_program.get_uniform_location(gl, "emissiveFactor");
		let use_glossiness_location = shader_program.get_uniform_location(gl, "useGlossiness");
		let use_alpha_cutout_location = shader_program.get_uniform_location(gl, "useAlphaCutout");
		let subsurface_factor_location = shader_program.get_uniform_location(gl, "subsurfaceFactor");
		let base_color_factor = self.base_color_factor;
		let emissive_factor = self.emissive_factor;
		unsafe {
			gl.uniform_3_f32(base_color_factor_location.as_ref(), base_color_factor[0], base_color_factor[1], base_color_factor[2]);
			gl.uniform_1_f32(metallic_factor_location.as_ref(), self.metallic_factor);
			gl.uniform_1_f32(roughness_factor_location.as_ref(), self.roughness_factor);
			gl.uniform_3_f32(emissive_factor_location.as_ref(), emissive_factor[0], emissive_factor[1], emissive_factor[2]);
			gl.uniform_1_i32(use_glossiness_location.as_ref(), i32::from(self.use_glossiness));
			gl.uniform_1_i32(use_alpha_cutout_location.as_ref(), i32::from(self.use_alpha_cutout.get()));
			gl.uniform_1_f32(subsurface_factor_location.as_ref(), self.subsurface_factor.get());
		}

		let texture_bindings = self.get_texture_bindings();
		for (uniform_name, texture_unit, texture) in texture_bindings.iter() {
			let uniform_location = shader_program.get_uniform_location(gl, uniform_name);
			unsafe {
				gl.uniform_1_i32(uniform_location.as_ref(), *texture_unit as i32);
				gl.active_texture(glow::TEXTURE0 + *texture_unit);
				gl.bind_texture(glow::TEXTURE_2D, *texture);
			}
		}
		unsafe {
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	/// 참조 셰이더 프로그램 반환. (머티리얼 템플릿의 셰이더)
	//==============================================================================
	pub fn get_shader_program(&self) -> &Rc<ShaderProgram> {
		&self.shader_program
	}

	//==============================================================================
	/// 베이스 컬러 팩터 반환.
	//==============================================================================
	pub fn get_base_color_factor(&self) -> [f32; 3] {
		self.base_color_factor
	}

	//==============================================================================
	/// 메탈릭 팩터 반환.
	//==============================================================================
	pub fn get_metallic_factor(&self) -> f32 {
		self.metallic_factor
	}

	//==============================================================================
	/// 러프니스 팩터 반환.
	//==============================================================================
	pub fn get_roughness_factor(&self) -> f32 {
		self.roughness_factor
	}

	//==============================================================================
	/// 이미시브 팩터 반환.
	//==============================================================================
	pub fn get_emissive_factor(&self) -> [f32; 3] {
		self.emissive_factor
	}

	//==============================================================================
	/// 글로시니스 경로 사용 여부 반환.
	//==============================================================================
	pub fn get_use_glossiness(&self) -> bool {
		self.use_glossiness
	}

	//==============================================================================
	/// 서브서피스 팩터 설정. (피부 셰이더의 화면 공간 산란 마스크 배율, 눈 / 치아처럼 산란하지 않는 부위는 0)
	//==============================================================================
	pub fn set_subsurface_factor(&self, subsurface_factor: f32) {
		self.subsurface_factor.set(subsurface_factor);
	}

	//==============================================================================
	/// 서브서피스 팩터 반환.
	//==============================================================================
	pub fn get_subsurface_factor(&self) -> f32 {
		self.subsurface_factor.get()
	}

	//==============================================================================
	/// 알파 컷아웃 사용 여부 설정. (오파시티 텍스처를 나중에 붙인 머티리얼용)
	//==============================================================================
	pub fn set_use_alpha_cutout(&self, use_alpha_cutout: bool) {
		self.use_alpha_cutout.set(use_alpha_cutout);
	}

	//==============================================================================
	/// 알파 컷아웃 사용 여부 반환. (오파시티 맵 보유 시, 머리카락/속눈썹 등)
	//==============================================================================
	pub fn get_use_alpha_cutout(&self) -> bool {
		self.use_alpha_cutout.get()
	}

	//==============================================================================
	/// 텍스처 바인딩 목록 반환. (유니폼 이름 / 텍스처 유닛 / GL 텍스처, 기본 슬롯 8개 뒤에 추가 슬롯)
	//==============================================================================
	pub fn get_texture_bindings(&self) -> Vec<(String, u32, Option<GlTexture>)> {
		let mut texture_bindings = vec![
			(String::from("baseColorTexture"), TEXTURE_UNIT_BASECOLOR, self.base_color_texture),
			(String::from("normalTexture"), TEXTURE_UNIT_NORMAL, self.normal_texture),
			(String::from("metallicRoughnessTexture"), TEXTURE_UNIT_METALLICROUGHNESS, self.metallic_roughness_texture),
			(String::from("glossinessTexture"), TEXTURE_UNIT_GLOSSINESS, self.glossiness_texture),
			(String::from("occlusionTexture"), TEXTURE_UNIT_OCCLUSION, self.occlusion_texture),
			(String::from("emissiveTexture"), TEXTURE_UNIT_EMISSIVE, self.emissive_texture),
			(String::from("specularTexture"), TEXTURE_UNIT_SPECULAR, self.specular_texture),
			(String::from("opacityTexture"), TEXTURE_UNIT_OPACITY, self.opacity_texture),
		];
		for binding in self.extra_texture_bindings.borrow().iter() {
			texture_bindings.push(binding.clone());
		}
		texture_bindings
	}

	//==============================================================================
	/// 파괴. (기본 슬롯 텍스처 해제, 추가 슬롯 텍스처는 건 쪽이 지운다)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		let owned_texture_list = [self.base_color_texture, self.normal_texture, self.metallic_roughness_texture, self.glossiness_texture, self.occlusion_texture, self.emissive_texture, self.specular_texture, self.opacity_texture];
		for texture in owned_texture_list.into_iter().flatten() {
			unsafe {
				gl.delete_texture(texture);
			}
		}
	}
}

//==============================================================================
/// 이미지 바이트 해석. (png / jpeg / webp → 너비, 높이, 스트레이트 알파 RGBA)
//==============================================================================
pub fn decode_image_bytes(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
	let dynamic_image = image::load_from_memory(bytes).map_err(|error| format!("Material: image decode failed: {}", error))?;
	let rgba_image = dynamic_image.to_rgba8();
	let width = rgba_image.width();
	let height = rgba_image.height();
	Ok((width, height, rgba_image.into_raw()))
}

//==============================================================================
/// 픽셀 행 위아래 뒤집기. (원본 createImageBitmap 의 imageOrientation: "flip_y")
//==============================================================================
pub fn flip_pixel_rows(pixels: &mut [u8], width: usize, height: usize) {
	let row_byte_count = width * 4;
	for row_index in 0..height / 2 {
		let mirror_row_index = height - 1 - row_index;
		let (upper_part, lower_part) = pixels.split_at_mut(mirror_row_index * row_byte_count);
		upper_part[row_index * row_byte_count..(row_index + 1) * row_byte_count].swap_with_slice(&mut lower_part[..row_byte_count]);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 이미지 서술 받기: mime_type 은 응답의 content-type, 없으면(데스크탑) 내용으로 고른 형. 없는 파일은 원본 문구의 Err.
	//==============================================================================
	#[test]
	fn image_description_load_handle() {
		let png_signature = vec![0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
		let file_load_handle = AssetLoadHandle::create_loaded("textures/skin.dat", png_signature.clone());
		let file_description_handle = ImageDescriptionLoadHandle {
			load_handle: file_load_handle,
			options: None,
		};
		let file_description = file_description_handle.get_image_description().expect("끝남").expect("서술");
		assert_eq!(file_description.mime_type.as_deref(), Some("image/png"));
		let response_load_handle = AssetLoadHandle::create_loaded("textures/skin.png", png_signature);
		response_load_handle.set_content_type(Some(String::from("application/octet-stream")));
		let response_description_handle = ImageDescriptionLoadHandle {
			load_handle: response_load_handle,
			options: None,
		};
		let response_description = response_description_handle.get_image_description().expect("끝남").expect("서술");
		assert_eq!(response_description.mime_type.as_deref(), Some("application/octet-stream"));
		let unknown_description_handle = ImageDescriptionLoadHandle {
			load_handle: AssetLoadHandle::create_loaded("unknown.bin", vec![1, 2, 3]),
			options: None,
		};
		assert_eq!(unknown_description_handle.get_image_description().expect("끝남").expect("서술").mime_type, None);
		let engine = Engine::new(crate::core::engine::EngineConfiguration::new());
		let missing_load_handle = Material::load_image_description(&engine, "missing_texture.png", None);
		assert!(missing_load_handle.is_finished());
		assert_eq!(missing_load_handle.get_image_description().map(|description_result| description_result.err()), Some(Some(String::from("Material image load failed: missing_texture.png"))));
	}

	//==============================================================================
	/// 위아래 뒤집기. (2x3 이미지의 행 순서가 거꾸로 된다)
	//==============================================================================
	#[test]
	fn flip_pixel_rows_reverses_rows() {
		let mut pixels: Vec<u8> = (0..24).collect();
		flip_pixel_rows(&mut pixels, 2, 3);
		let expected: Vec<u8> = vec![16, 17, 18, 19, 20, 21, 22, 23, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7];
		assert_eq!(pixels, expected);
	}

	//==============================================================================
	/// 기본 서술. (원본 create_default_description 값)
	//==============================================================================
	#[test]
	fn default_description_matches_original() {
		let description = Material::create_default_description();
		assert_eq!(description.base_color_factor, [1.0, 1.0, 1.0]);
		assert_eq!(description.metallic_factor, 0.0);
		assert_eq!(description.roughness_factor, 1.0);
		assert_eq!(description.emissive_factor, [0.0, 0.0, 0.0]);
		assert!(description.base_color_image.is_none() && description.opacity_image.is_none());
	}

	//==============================================================================
	/// 서술로 짓다가 중간 이미지를 못 풀면 앞에서 지은 텍스처를 지우고 오류를 돌려준다. (가짜 GL)
	//==============================================================================
	#[test]
	fn failed_description_deletes_created_textures() {
		use crate::core::imagetexturecache::fake_gl::{self, FakeGlObject};
		let gl = fake_gl::create_context();
		let shader_program = Rc::new(ShaderProgram::new(&gl, "", "").expect("shader program"));
		let mut description = Material::create_default_description();
		description.metallic_roughness_image = Some(Material::create_image_description(vec![1, 2, 3], None, None));
		assert!(Material::create_from_description(&gl, shader_program.clone(), &description).is_err());
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 0);
		let material = Material::create_from_description(&gl, shader_program, &Material::create_default_description()).expect("material");
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 8);
		material.destroy(&gl);
		assert_eq!(fake_gl::get_live_count(FakeGlObject::Texture), 0);
	}
}
