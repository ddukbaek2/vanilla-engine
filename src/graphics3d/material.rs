//==============================================================================
// 머티리얼. (원본 experimental/graphics/material.js)
// - 셰이더 템플릿 참조 + metallic-roughness PBR 파라미터/텍스처, 포맷 중립. 모델은 머티리얼 인스턴스를 적용만 한다.
// - 임포터가 만든 중립 머티리얼 서술(팩터 + 이미지 바이트, MaterialDescription)로부터 GL 텍스처를 만든다.
// - FBX 의 스펙큘러/글로스 맵은 glossiness 경로로 러프니스를 근사한다.
// - 텍스처 유닛 배치는 원본과 같다: 0 = baseColor, 1 = 섀도우 맵 전용(사용 금지), 2 ~ 8 = normal / metallicRoughness /
//   glossiness / occlusion / emissive / specular / opacity, 추가 슬롯은 9 이상.
// - 원본과 다른 것:
//   - 셰이더 프로그램은 Rc<ShaderProgram> 으로 렌더러와 나눠 든다. 모델의 드로어블은 Rc<Material> 로 가리킨다. (원본 참조 의미)
//     원본이 인스턴스를 고치는 setTexture / setUseAlphaCutout / setSubsurfaceFactor 는 &self 로 부를 수 있게 안에서 Cell 로 든다.
//   - 이미지 해석은 원본 createImageBitmap(비동기) 대신 image 크레이트(png, jpeg, webp)로 그 자리에서 한다. 그래서 async 가 없다.
//     원본처럼 flipY 면 위아래를 뒤집고, isColorData 면 SRGB8_ALPHA8 로 올린다.
//   - 원본 loadImageDescription(url) 은 데스크탑 경로를 읽는 loadImageDescription(path) 와 바이트를 받는
//     createImageDescription(bytes) 로 나눴다. (웹은 BlobAsset 으로 받은 바이트를 넘긴다)
//   - 원본 이미지 서술의 source(캔버스/비트맵)는 RGBA 픽셀(ImageSource)로 받는다.
//   - 원본에 없는 destroy 를 더했다. (텍스처를 지운다)
//==============================================================================
use crate::core::graphic::GlTexture;
use crate::core::shaderprogram::ShaderProgram;
use glow::HasContext;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 텍스처 유닛 배치. (1번은 SkinnedModel 의 섀도우 맵 전용, 사용 금지)
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
// 이미 푼 RGBA 픽셀. (원본 이미지 서술의 source, 캔버스/비트맵 자리, 스트레이트 알파, 위에서 아래로)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ImageSource {
	pub width: u32,
	pub height: u32,
	pub pixels: Vec<u8>,
}

//==============================================================================
// 이미지 서술. (원본 { bytes, mimeType, flipY, isColorData, anisotropy } 또는 { source, flipY })
// - bytes 는 png / jpeg / webp 파일 내용이다. 같은 이미지를 여러 텍스처가 쓰면 Rc 로 나눠 든다. (FBX 비디오 중복 참조)
//==============================================================================
#[derive(Clone, Debug, Default)]
pub struct ImageDescription {
	pub bytes: Rc<Vec<u8>>,
	pub mimeType: Option<String>,
	pub source: Option<Rc<ImageSource>>,
	pub flipY: bool,
	pub isColorData: bool,
	pub anisotropy: f32,
}

//==============================================================================
// 이미지 서술 옵션. (원본 loadImageDescription 의 options { flipY, isColorData, anisotropy })
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageDescriptionOptions {
	pub flipY: bool,
	pub isColorData: bool,
	pub anisotropy: f32,
}

//==============================================================================
// 중립 머티리얼 서술. (원본 createDefaultDescription 이 만드는 객체, 팩터 + 이미지, GL 무관)
//==============================================================================
#[derive(Clone, Debug)]
pub struct MaterialDescription {
	pub baseColorFactor: [f32; 3],
	pub metallicFactor: f32,
	pub roughnessFactor: f32,
	pub emissiveFactor: [f32; 3],
	pub baseColorImage: Option<ImageDescription>,
	pub normalImage: Option<ImageDescription>,
	pub metallicRoughnessImage: Option<ImageDescription>,
	pub glossinessImage: Option<ImageDescription>,
	pub occlusionImage: Option<ImageDescription>,
	pub emissiveImage: Option<ImageDescription>,
	pub specularImage: Option<ImageDescription>,
	pub opacityImage: Option<ImageDescription>,
}

//==============================================================================
// 머티리얼.
//==============================================================================
pub struct Material {
	shaderProgram: Rc<ShaderProgram>,
	baseColorFactor: [f32; 3],
	metallicFactor: f32,
	roughnessFactor: f32,
	emissiveFactor: [f32; 3],
	useGlossiness: bool,
	useAlphaCutout: Cell<bool>,
	baseColorTexture: Option<GlTexture>,
	normalTexture: Option<GlTexture>,
	metallicRoughnessTexture: Option<GlTexture>,
	glossinessTexture: Option<GlTexture>,
	occlusionTexture: Option<GlTexture>,
	emissiveTexture: Option<GlTexture>,
	specularTexture: Option<GlTexture>,
	opacityTexture: Option<GlTexture>,
	extraTextureBindings: RefCell<Vec<(String, u32, Option<GlTexture>)>>,
	subsurfaceFactor: Cell<f32>,
}

impl Material {
	//==============================================================================
	// 생성. (텍스처 없음, 흰색 팩터, 메탈릭 1, 러프니스 1)
	//==============================================================================
	pub fn new(shaderProgram: Rc<ShaderProgram>) -> Material {
		Material {
			shaderProgram,
			baseColorFactor: [1.0, 1.0, 1.0],
			metallicFactor: 1.0,
			roughnessFactor: 1.0,
			emissiveFactor: [0.0, 0.0, 0.0],
			useGlossiness: false,
			useAlphaCutout: Cell::new(false),
			baseColorTexture: None,
			normalTexture: None,
			metallicRoughnessTexture: None,
			glossinessTexture: None,
			occlusionTexture: None,
			emissiveTexture: None,
			specularTexture: None,
			opacityTexture: None,
			extraTextureBindings: RefCell::new(Vec::new()),
			subsurfaceFactor: Cell::new(1.0),
		}
	}

	//==============================================================================
	// 기본 머티리얼 서술 생성. (흰색 무광 비금속)
	//==============================================================================
	pub fn createDefaultDescription() -> MaterialDescription {
		MaterialDescription {
			baseColorFactor: [1.0, 1.0, 1.0],
			metallicFactor: 0.0,
			roughnessFactor: 1.0,
			emissiveFactor: [0.0, 0.0, 0.0],
			baseColorImage: None,
			normalImage: None,
			metallicRoughnessImage: None,
			glossinessImage: None,
			occlusionImage: None,
			emissiveImage: None,
			specularImage: None,
			opacityImage: None,
		}
	}

	//==============================================================================
	// 중립 머티리얼 서술로부터 생성. (이미지 바이트 → GL 텍스처, 못 풀면 오류)
	//==============================================================================
	pub fn createFromDescription(gl: &glow::Context, shaderProgram: Rc<ShaderProgram>, description: &MaterialDescription) -> Result<Material, String> {
		let mut material = Material::new(shaderProgram);
		material.baseColorFactor = description.baseColorFactor;
		material.metallicFactor = description.metallicFactor;
		material.roughnessFactor = description.roughnessFactor;
		material.emissiveFactor = description.emissiveFactor;
		material.useGlossiness = description.glossinessImage.is_some();
		material.useAlphaCutout.set(description.opacityImage.is_some());
		material.baseColorTexture = Some(Material::createTexture(gl, description.baseColorImage.as_ref(), [255, 255, 255, 255])?);
		material.normalTexture = Some(Material::createTexture(gl, description.normalImage.as_ref(), [128, 128, 255, 255])?);
		material.metallicRoughnessTexture = Some(Material::createTexture(gl, description.metallicRoughnessImage.as_ref(), [255, 255, 255, 255])?);
		material.glossinessTexture = Some(Material::createTexture(gl, description.glossinessImage.as_ref(), [255, 255, 255, 255])?);
		material.occlusionTexture = Some(Material::createTexture(gl, description.occlusionImage.as_ref(), [255, 255, 255, 255])?);
		material.emissiveTexture = Some(Material::createTexture(gl, description.emissiveImage.as_ref(), [255, 255, 255, 255])?);
		material.specularTexture = Some(Material::createTexture(gl, description.specularImage.as_ref(), [255, 255, 255, 255])?);
		material.opacityTexture = Some(Material::createTexture(gl, description.opacityImage.as_ref(), [255, 255, 255, 255])?);
		Ok(material)
	}

	//==============================================================================
	// 텍스처 생성. (이미지 서술 → GL 텍스처, 없으면 1x1 단색 대체)
	// - isColorData: sRGB 로 저장된 색 텍스처, 하드웨어 디코드(SRGB8_ALPHA8)로 선형 공간 샘플링.
	// - anisotropy: 이방성 필터 단계. (확장 미지원 시 무시)
	//==============================================================================
	pub fn createTexture(gl: &glow::Context, imageDescription: Option<&ImageDescription>, fallbackColor: [u8; 4]) -> Result<GlTexture, String> {
		let hasBytes = imageDescription.map(|description| !description.bytes.is_empty()).unwrap_or(false);
		let hasSource = imageDescription.map(|description| description.source.is_some()).unwrap_or(false);
		let imageDescription = match imageDescription {
			Some(imageDescription) if hasBytes || hasSource => {
				imageDescription
			}
			_ => {
				return unsafe {
					let texture = gl.create_texture()?;
					gl.bind_texture(glow::TEXTURE_2D, Some(texture));
					gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::RGBA8 as i32, 1, 1, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&fallbackColor)));
					Ok(texture)
				};
			}
		};

		// 픽셀 준비. (원본 createImageBitmap, flipY 면 위아래를 뒤집는다)
		let (width, height, mut pixels) = match &imageDescription.source {
			Some(imageSource) => {
				(imageSource.width, imageSource.height, imageSource.pixels.clone())
			}
			None => {
				decodeImageBytes(&imageDescription.bytes)?
			}
		};
		if pixels.len() < (width as usize) * (height as usize) * 4 {
			return Err(format!("Material: image pixel count mismatch ({}x{})", width, height));
		}
		if imageDescription.flipY {
			flipPixelRows(&mut pixels, width as usize, height as usize);
		}
		let internalFormat = if imageDescription.isColorData {
			glow::SRGB8_ALPHA8
		}
		else {
			glow::RGBA8
		};
		unsafe {
			let texture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(texture));
			gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
			gl.tex_image_2d(glow::TEXTURE_2D, 0, internalFormat as i32, width as i32, height as i32, 0, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelUnpackData::Slice(Some(&pixels)));
			gl.generate_mipmap(glow::TEXTURE_2D);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR_MIPMAP_LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::REPEAT as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::REPEAT as i32);
			if imageDescription.anisotropy > 0.0 {
				let hasAnisotropicExtension = gl.supported_extensions().iter().any(|extensionName| extensionName.contains("texture_filter_anisotropic"));
				if hasAnisotropicExtension {
					let maximumAnisotropy = gl.get_parameter_f32(glow::MAX_TEXTURE_MAX_ANISOTROPY_EXT);
					let anisotropy = imageDescription.anisotropy.min(maximumAnisotropy);
					gl.tex_parameter_f32(glow::TEXTURE_2D, glow::TEXTURE_MAX_ANISOTROPY_EXT, anisotropy);
				}
			}
			Ok(texture)
		}
	}

	//==============================================================================
	// 바이트로 이미지 서술 생성. (원본 loadImageDescription 의 fetch 뒤 부분, 웹은 BlobAsset 바이트를 넘긴다)
	//==============================================================================
	pub fn createImageDescription(bytes: Vec<u8>, mimeType: Option<String>, options: Option<ImageDescriptionOptions>) -> ImageDescription {
		let resolvedOptions = options.unwrap_or_default();
		ImageDescription {
			bytes: Rc::new(bytes),
			mimeType,
			source: None,
			flipY: resolvedOptions.flipY,
			isColorData: resolvedOptions.isColorData,
			anisotropy: resolvedOptions.anisotropy,
		}
	}

	//==============================================================================
	// 파일로부터 이미지 서술 로드. (원본 loadImageDescription(url) 의 데스크탑 판, mimeType 은 확장자로 정한다)
	//==============================================================================
	#[cfg(not(target_arch = "wasm32"))]
	pub fn loadImageDescription(path: &std::path::Path, options: Option<ImageDescriptionOptions>) -> Result<ImageDescription, String> {
		let bytes = std::fs::read(path).map_err(|error| format!("Material image load failed: {} ({})", path.display(), error))?;
		let extension = path.extension().map(|extension| extension.to_string_lossy().to_lowercase()).unwrap_or_default();
		let mimeType = match extension.as_str() {
			"png" => {
				Some(String::from("image/png"))
			}
			"jpg" | "jpeg" => {
				Some(String::from("image/jpeg"))
			}
			"webp" => {
				Some(String::from("image/webp"))
			}
			_ => {
				None
			}
		};
		Ok(Material::createImageDescription(bytes, mimeType, options))
	}

	//==============================================================================
	// 추가 텍스처 설정. (셰이더 템플릿 고유 슬롯, 기본 슬롯 밖의 유니폼 이름 / 텍스처 유닛)
	// - 유닛 1 은 섀도우 맵 전용, 0 / 2~8 은 기본 슬롯이므로 9 이상을 사용한다. 같은 이름은 자리를 지키고 값만 바꾼다.
	//==============================================================================
	pub fn setTexture(&self, uniformName: &str, textureUnit: u32, texture: Option<GlTexture>) {
		let mut extraTextureBindings = self.extraTextureBindings.borrow_mut();
		for binding in extraTextureBindings.iter_mut() {
			if binding.0 == uniformName {
				binding.1 = textureUnit;
				binding.2 = texture;
				return;
			}
		}
		extraTextureBindings.push((uniformName.to_string(), textureUnit, texture));
	}

	//==============================================================================
	// 적용. (참조 셰이더에 유니폼 설정 + 텍스처 유닛 바인드)
	//==============================================================================
	pub fn apply(&self, gl: &glow::Context) {
		let shaderProgram = &self.shaderProgram;
		let baseColorFactorLocation = shaderProgram.getUniformLocation(gl, "baseColorFactor");
		let metallicFactorLocation = shaderProgram.getUniformLocation(gl, "metallicFactor");
		let roughnessFactorLocation = shaderProgram.getUniformLocation(gl, "roughnessFactor");
		let emissiveFactorLocation = shaderProgram.getUniformLocation(gl, "emissiveFactor");
		let useGlossinessLocation = shaderProgram.getUniformLocation(gl, "useGlossiness");
		let useAlphaCutoutLocation = shaderProgram.getUniformLocation(gl, "useAlphaCutout");
		let subsurfaceFactorLocation = shaderProgram.getUniformLocation(gl, "subsurfaceFactor");
		let baseColorFactor = self.baseColorFactor;
		let emissiveFactor = self.emissiveFactor;
		unsafe {
			gl.uniform_3_f32(baseColorFactorLocation.as_ref(), baseColorFactor[0], baseColorFactor[1], baseColorFactor[2]);
			gl.uniform_1_f32(metallicFactorLocation.as_ref(), self.metallicFactor);
			gl.uniform_1_f32(roughnessFactorLocation.as_ref(), self.roughnessFactor);
			gl.uniform_3_f32(emissiveFactorLocation.as_ref(), emissiveFactor[0], emissiveFactor[1], emissiveFactor[2]);
			gl.uniform_1_i32(useGlossinessLocation.as_ref(), i32::from(self.useGlossiness));
			gl.uniform_1_i32(useAlphaCutoutLocation.as_ref(), i32::from(self.useAlphaCutout.get()));
			gl.uniform_1_f32(subsurfaceFactorLocation.as_ref(), self.subsurfaceFactor.get());
		}

		let textureBindings = self.getTextureBindings();
		for (uniformName, textureUnit, texture) in textureBindings.iter() {
			let uniformLocation = shaderProgram.getUniformLocation(gl, uniformName);
			unsafe {
				gl.uniform_1_i32(uniformLocation.as_ref(), *textureUnit as i32);
				gl.active_texture(glow::TEXTURE0 + *textureUnit);
				gl.bind_texture(glow::TEXTURE_2D, *texture);
			}
		}
		unsafe {
			gl.active_texture(glow::TEXTURE0);
		}
	}

	//==============================================================================
	// 참조 셰이더 프로그램 반환. (머티리얼 템플릿의 셰이더)
	//==============================================================================
	pub fn getShaderProgram(&self) -> &Rc<ShaderProgram> {
		&self.shaderProgram
	}

	//==============================================================================
	// 베이스 컬러 팩터 반환.
	//==============================================================================
	pub fn getBaseColorFactor(&self) -> [f32; 3] {
		self.baseColorFactor
	}

	//==============================================================================
	// 메탈릭 팩터 반환.
	//==============================================================================
	pub fn getMetallicFactor(&self) -> f32 {
		self.metallicFactor
	}

	//==============================================================================
	// 러프니스 팩터 반환.
	//==============================================================================
	pub fn getRoughnessFactor(&self) -> f32 {
		self.roughnessFactor
	}

	//==============================================================================
	// 이미시브 팩터 반환.
	//==============================================================================
	pub fn getEmissiveFactor(&self) -> [f32; 3] {
		self.emissiveFactor
	}

	//==============================================================================
	// 글로시니스 경로 사용 여부 반환.
	//==============================================================================
	pub fn getUseGlossiness(&self) -> bool {
		self.useGlossiness
	}

	//==============================================================================
	// 서브서피스 팩터 설정. (피부 셰이더의 화면 공간 산란 마스크 배율, 눈 / 치아처럼 산란하지 않는 부위는 0)
	//==============================================================================
	pub fn setSubsurfaceFactor(&self, subsurfaceFactor: f32) {
		self.subsurfaceFactor.set(subsurfaceFactor);
	}

	//==============================================================================
	// 서브서피스 팩터 반환.
	//==============================================================================
	pub fn getSubsurfaceFactor(&self) -> f32 {
		self.subsurfaceFactor.get()
	}

	//==============================================================================
	// 알파 컷아웃 사용 여부 설정. (오파시티 텍스처를 나중에 붙인 머티리얼용)
	//==============================================================================
	pub fn setUseAlphaCutout(&self, useAlphaCutout: bool) {
		self.useAlphaCutout.set(useAlphaCutout);
	}

	//==============================================================================
	// 알파 컷아웃 사용 여부 반환. (오파시티 맵 보유 시, 머리카락/속눈썹 등)
	//==============================================================================
	pub fn getUseAlphaCutout(&self) -> bool {
		self.useAlphaCutout.get()
	}

	//==============================================================================
	// 텍스처 바인딩 목록 반환. (유니폼 이름 / 텍스처 유닛 / GL 텍스처, 기본 슬롯 8개 뒤에 추가 슬롯)
	//==============================================================================
	pub fn getTextureBindings(&self) -> Vec<(String, u32, Option<GlTexture>)> {
		let mut textureBindings = vec![
			(String::from("baseColorTexture"), TEXTURE_UNIT_BASECOLOR, self.baseColorTexture),
			(String::from("normalTexture"), TEXTURE_UNIT_NORMAL, self.normalTexture),
			(String::from("metallicRoughnessTexture"), TEXTURE_UNIT_METALLICROUGHNESS, self.metallicRoughnessTexture),
			(String::from("glossinessTexture"), TEXTURE_UNIT_GLOSSINESS, self.glossinessTexture),
			(String::from("occlusionTexture"), TEXTURE_UNIT_OCCLUSION, self.occlusionTexture),
			(String::from("emissiveTexture"), TEXTURE_UNIT_EMISSIVE, self.emissiveTexture),
			(String::from("specularTexture"), TEXTURE_UNIT_SPECULAR, self.specularTexture),
			(String::from("opacityTexture"), TEXTURE_UNIT_OPACITY, self.opacityTexture),
		];
		for binding in self.extraTextureBindings.borrow().iter() {
			textureBindings.push(binding.clone());
		}
		textureBindings
	}

	//==============================================================================
	// 파괴. (기본 슬롯 텍스처 해제, 원본에 없는 것, 추가 슬롯 텍스처는 건 쪽이 지운다)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		let ownedTextureList = [self.baseColorTexture, self.normalTexture, self.metallicRoughnessTexture, self.glossinessTexture, self.occlusionTexture, self.emissiveTexture, self.specularTexture, self.opacityTexture];
		for texture in ownedTextureList.into_iter().flatten() {
			unsafe {
				gl.delete_texture(texture);
			}
		}
	}
}

//==============================================================================
// 이미지 바이트 해석. (png / jpeg / webp → 너비, 높이, 스트레이트 알파 RGBA)
//==============================================================================
pub fn decodeImageBytes(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
	let dynamicImage = image::load_from_memory(bytes).map_err(|error| format!("Material: image decode failed: {}", error))?;
	let rgbaImage = dynamicImage.to_rgba8();
	let width = rgbaImage.width();
	let height = rgbaImage.height();
	Ok((width, height, rgbaImage.into_raw()))
}

//==============================================================================
// 픽셀 행 위아래 뒤집기. (원본 createImageBitmap 의 imageOrientation: "flipY")
//==============================================================================
pub fn flipPixelRows(pixels: &mut [u8], width: usize, height: usize) {
	let rowByteCount = width * 4;
	for rowIndex in 0..height / 2 {
		let mirrorRowIndex = height - 1 - rowIndex;
		let (upperPart, lowerPart) = pixels.split_at_mut(mirrorRowIndex * rowByteCount);
		upperPart[rowIndex * rowByteCount..(rowIndex + 1) * rowByteCount].swap_with_slice(&mut lowerPart[..rowByteCount]);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 위아래 뒤집기. (2x3 이미지의 행 순서가 거꾸로 된다)
	//==============================================================================
	#[test]
	fn flipPixelRowsReversesRows() {
		let mut pixels: Vec<u8> = (0..24).collect();
		flipPixelRows(&mut pixels, 2, 3);
		let expected: Vec<u8> = vec![16, 17, 18, 19, 20, 21, 22, 23, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7];
		assert_eq!(pixels, expected);
	}

	//==============================================================================
	// 기본 서술. (원본 createDefaultDescription 값)
	//==============================================================================
	#[test]
	fn defaultDescriptionMatchesOriginal() {
		let description = Material::createDefaultDescription();
		assert_eq!(description.baseColorFactor, [1.0, 1.0, 1.0]);
		assert_eq!(description.metallicFactor, 0.0);
		assert_eq!(description.roughnessFactor, 1.0);
		assert_eq!(description.emissiveFactor, [0.0, 0.0, 0.0]);
		assert!(description.baseColorImage.is_none() && description.opacityImage.is_none());
	}
}
