//==============================================================================
// 섀도우 맵. (원본 experimental/graphics/shadowmap.js)
// - 방향광 실시간 그림자, 깊이 텍스처(하드웨어 비교 샘플링, sampler2DShadow) + 직교 광원 행렬.
// - beginRender() 로 깊이 패스를 시작하고, 결과 텍스처를 본 패스에서 샘플링한다.
// - 광원 행렬은 텍셀 단위로 스냅해 이동 시 그림자 가장자리 떨림을 줄인다.
// - 텍셀 스냅의 반올림은 원본 Math.round 와 같이 0.5 를 위로 올린다. (러스트 round 는 0 에서 먼 쪽이라 음수에서 다르다)
// - 원본에 없는 destroy 를 더했다. 원본 해상도 기본값 2048 은 DEFAULT_SHADOWMAP_RESOLUTION 으로 둔다.
//==============================================================================
use crate::base::matrix4::Matrix4;
use crate::base::vector3::Vector3;
use crate::core::graphic::GlTexture;
use crate::graphics3d::GlFramebuffer;
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
// 해상도 기본값. (원본 생성자 기본 인자)
pub const DEFAULT_SHADOWMAP_RESOLUTION: i32 = 2048;

//==============================================================================
// 섀도우 맵.
//==============================================================================
pub struct ShadowMap {
	framebuffer: GlFramebuffer,
	depthTexture: GlTexture,
	resolution: i32,
	lightViewProjectionMatrix: Matrix4,
}

impl ShadowMap {
	//==============================================================================
	// 생성. (깊이 텍스처 + 컬러 없는 프레임버퍼)
	//==============================================================================
	pub fn new(gl: &glow::Context, resolution: i32) -> Result<ShadowMap, String> {
		unsafe {
			// 깊이 텍스처. (하드웨어 비교 샘플링, sampler2DShadow)
			let depthTexture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(depthTexture));
			gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, resolution, resolution, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
			gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);

			// 프레임버퍼. (컬러 어태치먼트 없음, 깊이 전용)
			let framebuffer = gl.create_framebuffer()?;
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depthTexture), 0);
			gl.draw_buffers(&[glow::NONE]);
			gl.read_buffer(glow::NONE);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);

			Ok(ShadowMap {
				framebuffer,
				depthTexture,
				resolution,
				lightViewProjectionMatrix: Matrix4::createIdentity(),
			})
		}
	}

	//==============================================================================
	// 광원 행렬 갱신. (초점 주변을 덮는 직교 투영, 텍셀 스냅)
	// - lightDirection 은 빛이 나아가는 방향, extent 는 초점 기준 절반 커버 범위다.
	//==============================================================================
	pub fn updateLightMatrix(&mut self, focusPosition: Vector3, lightDirection: Vector3, extent: f32, nearDistance: f32, farDistance: f32) {
		let normalizedDirection = lightDirection.normalize();
		let eyeDistance = (nearDistance + farDistance) * 0.5;
		let eyeOffset = normalizedDirection.multiplyScalar(-eyeDistance);
		let eyePosition = focusPosition.add(eyeOffset);
		let upDirection = Vector3::up();
		let mut viewMatrix = Matrix4::createLookAt(eyePosition, focusPosition, upDirection);

		// 텍셀 스냅. (광원 공간 이동 성분을 텍셀 크기로 양자화, 떨림 방지)
		let worldUnitsPerTexel = (extent * 2.0) / self.resolution as f32;
		let viewElements = &mut viewMatrix.elements;
		viewElements[12] = roundHalfUp(viewElements[12] / worldUnitsPerTexel) * worldUnitsPerTexel;
		viewElements[13] = roundHalfUp(viewElements[13] / worldUnitsPerTexel) * worldUnitsPerTexel;

		let orthographicMatrix = Matrix4::createOrthographic(-extent, extent, -extent, extent, nearDistance, farDistance);
		let mut lightViewProjectionMatrix = orthographicMatrix;
		lightViewProjectionMatrix.multiply(&viewMatrix);
		self.lightViewProjectionMatrix = lightViewProjectionMatrix;
	}

	//==============================================================================
	// 깊이 패스 시작. (프레임버퍼 바인드 + 뷰포트 + 깊이 클리어)
	//==============================================================================
	pub fn beginRender(&self, gl: &glow::Context) {
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
			gl.viewport(0, 0, self.resolution, self.resolution);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_mask(true);
			gl.clear(glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	// 프레임버퍼 반환.
	//==============================================================================
	pub fn getFramebuffer(&self) -> GlFramebuffer {
		self.framebuffer
	}

	//==============================================================================
	// 깊이 텍스처 반환.
	//==============================================================================
	pub fn getDepthTexture(&self) -> GlTexture {
		self.depthTexture
	}

	//==============================================================================
	// 해상도 반환.
	//==============================================================================
	pub fn getResolution(&self) -> i32 {
		self.resolution
	}

	//==============================================================================
	// 광원 뷰 투영 행렬 반환.
	//==============================================================================
	pub fn getLightViewProjectionMatrix(&self) -> &Matrix4 {
		&self.lightViewProjectionMatrix
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제, 원본에 없는 것)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_framebuffer(self.framebuffer);
			gl.delete_texture(self.depthTexture);
		}
	}
}

//==============================================================================
// 반올림. (원본 Math.round 와 같이 x.5 는 위쪽 정수로)
//==============================================================================
fn roundHalfUp(value: f32) -> f32 {
	(value + 0.5).floor()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 반올림이 원본 Math.round 와 같은지. (Math.round(-2.5) = -2, Math.round(2.5) = 3)
	//==============================================================================
	#[test]
	fn roundHalfUpMatchesJavaScriptRound() {
		assert_eq!(roundHalfUp(2.5), 3.0);
		assert_eq!(roundHalfUp(-2.5), -2.0);
		assert_eq!(roundHalfUp(-2.6), -3.0);
		assert_eq!(roundHalfUp(1.49), 1.0);
	}
}
