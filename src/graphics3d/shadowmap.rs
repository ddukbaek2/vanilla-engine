//==============================================================================
// 섀도우 맵. (원본 experimental/graphics/shadowmap.js)
// - 방향광 실시간 그림자, 깊이 텍스처(하드웨어 비교 샘플링, sampler2DShadow) + 직교 광원 행렬.
// - begin_render() 로 깊이 패스를 시작하고, 결과 텍스처를 본 패스에서 샘플링한다.
// - 광원 행렬은 텍셀 단위로 스냅해 이동 시 그림자 가장자리 떨림을 줄인다.
// - 텍셀 스냅의 반올림은 원본 Math.round 와 같이 0.5 를 위로 올린다. (러스트 round 는 0 에서 먼 쪽이라 음수에서 다르다)
// - 원본 해상도 기본값 인자 2048 은 DEFAULT_SHADOWMAP_RESOLUTION 으로 둔다. (러스트에 기본값 인자가 없다)
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
	depth_texture: GlTexture,
	resolution: i32,
	light_view_projection_matrix: Matrix4,
}

impl ShadowMap {
	//==============================================================================
	// 생성. (깊이 텍스처 + 컬러 없는 프레임버퍼)
	//==============================================================================
	pub fn new(gl: &glow::Context, resolution: i32) -> Result<ShadowMap, String> {
		unsafe {
			// 깊이 텍스처. (하드웨어 비교 샘플링, sampler2DShadow)
			let depth_texture = gl.create_texture()?;
			gl.bind_texture(glow::TEXTURE_2D, Some(depth_texture));
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
			gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth_texture), 0);
			gl.draw_buffers(&[glow::NONE]);
			gl.read_buffer(glow::NONE);
			gl.bind_framebuffer(glow::FRAMEBUFFER, None);

			Ok(ShadowMap {
				framebuffer,
				depth_texture,
				resolution,
				light_view_projection_matrix: Matrix4::create_identity(),
			})
		}
	}

	//==============================================================================
	// 광원 행렬 갱신. (초점 주변을 덮는 직교 투영, 텍셀 스냅)
	// - light_direction 은 빛이 나아가는 방향, extent 는 초점 기준 절반 커버 범위다.
	//==============================================================================
	pub fn update_light_matrix(&mut self, focus_position: Vector3, light_direction: Vector3, extent: f32, near_distance: f32, far_distance: f32) {
		let normalized_direction = light_direction.normalize();
		let eye_distance = (near_distance + far_distance) * 0.5;
		let eye_offset = normalized_direction.multiply_scalar(-eye_distance);
		let eye_position = focus_position.add(eye_offset);
		let up_direction = Vector3::up();
		let mut view_matrix = Matrix4::create_look_at(eye_position, focus_position, up_direction);

		// 텍셀 스냅. (광원 공간 이동 성분을 텍셀 크기로 양자화, 떨림 방지)
		let world_units_per_texel = (extent * 2.0) / self.resolution as f32;
		let view_elements = &mut view_matrix.elements;
		view_elements[12] = round_half_up(view_elements[12] / world_units_per_texel) * world_units_per_texel;
		view_elements[13] = round_half_up(view_elements[13] / world_units_per_texel) * world_units_per_texel;

		let orthographic_matrix = Matrix4::create_orthographic(-extent, extent, -extent, extent, near_distance, far_distance);
		let mut light_view_projection_matrix = orthographic_matrix;
		light_view_projection_matrix.multiply(&view_matrix);
		self.light_view_projection_matrix = light_view_projection_matrix;
	}

	//==============================================================================
	// 깊이 패스 시작. (프레임버퍼 바인드 + 뷰포트 + 깊이 클리어)
	//==============================================================================
	pub fn begin_render(&self, gl: &glow::Context) {
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
	pub fn get_framebuffer(&self) -> GlFramebuffer {
		self.framebuffer
	}

	//==============================================================================
	// 깊이 텍스처 반환.
	//==============================================================================
	pub fn get_depth_texture(&self) -> GlTexture {
		self.depth_texture
	}

	//==============================================================================
	// 해상도 반환.
	//==============================================================================
	pub fn get_resolution(&self) -> i32 {
		self.resolution
	}

	//==============================================================================
	// 광원 뷰 투영 행렬 반환.
	//==============================================================================
	pub fn get_light_view_projection_matrix(&self) -> &Matrix4 {
		&self.light_view_projection_matrix
	}

	//==============================================================================
	// 파괴. (GL 리소스 해제)
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_framebuffer(self.framebuffer);
			gl.delete_texture(self.depth_texture);
		}
	}
}

//==============================================================================
// 반올림. (원본 Math.round 와 같이 x.5 는 위쪽 정수로)
//==============================================================================
fn round_half_up(value: f32) -> f32 {
	(value + 0.5).floor()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 반올림이 원본 Math.round 와 같은지. (Math.round(-2.5) = -2, Math.round(2.5) = 3)
	//==============================================================================
	#[test]
	fn round_half_up_matches_java_script_round() {
		assert_eq!(round_half_up(2.5), 3.0);
		assert_eq!(round_half_up(-2.5), -2.0);
		assert_eq!(round_half_up(-2.6), -3.0);
		assert_eq!(round_half_up(1.49), 1.0);
	}
}
