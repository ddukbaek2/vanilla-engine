//==============================================================================
//! 섀도우 맵. (원본 experimental/graphics/shadowmap.js)
//! - 방향광 실시간 그림자, 깊이 텍스처(하드웨어 비교 샘플링, sampler2DShadow) + 직교 광원 행렬.
//! - begin_render() 로 깊이 패스를 시작하고, 결과 텍스처를 본 패스에서 샘플링한다.
//! - 광원 행렬은 텍셀 단위로 스냅해 이동 시 그림자 가장자리 떨림을 줄인다.
//! - 텍셀 스냅의 반올림은 원본 Math.round 와 같이 0.5 를 위로 올린다. (러스트 round 는 0 에서 먼 쪽이라 음수에서 다르다)
//! - 원본 해상도 기본값 인자 2048 은 DEFAULT_SHADOWMAP_RESOLUTION 으로 둔다. (러스트에 기본값 인자가 없다)
//==============================================================================
use crate::base::matrix4::Matrix4;
use crate::base::vector3::Vector3;
use crate::core::graphic::GlTexture;
use crate::graphics3d::GlFramebuffer;
use glow::HasContext;

//==============================================================================
/// 전역 상수 목록.
//==============================================================================
/// 해상도 기본값. (원본 생성자 기본 인자)
pub const DEFAULT_SHADOWMAP_RESOLUTION: i32 = 2048;

//==============================================================================
/// 섀도우 맵.
/// - 깊이 텍스처는 정사각형 DEPTH_COMPONENT24 이고 비교 모드(LEQUAL)라 셰이더에서 sampler2DShadow 로 읽는다.
///
/// # 예제
/// 한 프레임의 그림자 깊이 패스를 그리고 본 패스 렌더러에 섀도우 맵 넘기기
/// ```rust,no_run
/// use vanilla::graphics3d::shadowmap::DEFAULT_SHADOWMAP_RESOLUTION;
/// use vanilla::{Matrix4, ShadowMap, SkinnedModel, SkinnedModelRenderer, Vector3};
///
/// fn create_shadow_map(gl: &glow::Context) -> Result<ShadowMap, String> {
/// 	ShadowMap::new(gl, DEFAULT_SHADOWMAP_RESOLUTION)
/// }
///
/// fn render_shadow_pass(gl: &glow::Context, shadow_map: &mut ShadowMap, skinned_model_renderer: &mut SkinnedModelRenderer, skinned_model: &mut SkinnedModel) {
/// 	let light_direction = Vector3::create(-0.5, -0.8, -0.35);
/// 	shadow_map.update_light_matrix(Vector3::zero(), light_direction, 4.0, 0.5, 30.0);
/// 	let light_view_projection_elements = *shadow_map.get_light_view_projection_matrix().get_elements();
/// 	let model_matrix = Matrix4::create_identity();
/// 	shadow_map.begin_render(gl);
/// 	skinned_model_renderer.draw_depth(gl, skinned_model, &light_view_projection_elements, model_matrix.get_elements());
/// 	let texel_size = 1.0 / shadow_map.get_resolution() as f32;
/// 	skinned_model_renderer.set_shadow_map(Some(shadow_map.get_depth_texture()), Some(&light_view_projection_elements), texel_size);
/// }
/// ```
//==============================================================================
pub struct ShadowMap {
	framebuffer: GlFramebuffer,
	depth_texture: GlTexture,
	resolution: i32,
	light_view_projection_matrix: Matrix4,
}

impl ShadowMap {
	//==============================================================================
	/// 생성. (깊이 텍스처 + 컬러 없는 프레임버퍼)
	/// - 광원 행렬은 단위 행렬로 시작한다. (update_light_matrix 로 채운다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트. (Graphic::get_gl 로 꺼낸다)
	/// - `resolution`: 깊이 텍스처 한 변의 텍셀 수. (정사각형, 원본 기본값은 DEFAULT_SHADOWMAP_RESOLUTION = 2048)
	///
	/// # 돌려줌
	/// 새 섀도우 맵. 텍스처나 프레임버퍼를 못 만들면 Err(GL 의 오류 글).
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
	/// 광원 행렬 갱신. (초점 주변을 덮는 직교 투영, 텍셀 스냅)
	/// - light_direction 은 빛이 나아가는 방향, extent 는 초점 기준 절반 커버 범위다.
	/// - 광원 눈은 초점에서 빛 반대쪽으로 (near_distance + far_distance) / 2 만큼 떨어지고, 위쪽은 Vector3::up() 이다.
	///   그래서 빛이 y 축과 나란하면(바로 위나 아래로) 위쪽과 겹쳐 행렬이 망가진다. (원본과 같음)
	///
	/// # 인자
	/// - `focus_position`: 그림자를 덮을 가운데. (월드)
	/// - `light_direction`: 빛이 나아가는 방향. (월드, 길이는 상관없다, 안에서 정규화)
	/// - `extent`: 초점에서 가로와 세로로 덮는 절반 범위. (월드 단위, 직교 상자는 -extent ~ extent)
	/// - `near_distance`: 광원 직교 투영의 가까운 거리. (월드 단위)
	/// - `far_distance`: 광원 직교 투영의 먼 거리. (월드 단위)
	///
	/// # 예제
	/// 해가 비스듬히 비출 때 원점 둘레 반지름 4 를 덮기
	/// ```rust,no_run
	/// use vanilla::{ShadowMap, Vector3};
	///
	/// fn aim_sun(shadow_map: &mut ShadowMap) {
	/// 	let sun_direction = Vector3::create(0.5, 0.8, 0.35).normalize();
	/// 	let light_travel_direction = sun_direction.multiply_scalar(-1.0);
	/// 	shadow_map.update_light_matrix(Vector3::zero(), light_travel_direction, 4.0, 0.5, 30.0);
	/// }
	/// ```
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
	/// 깊이 패스 시작. (프레임버퍼 바인드 + 뷰포트 + 깊이 클리어)
	/// - 깊이 시험과 깊이 쓰기를 켠다. 끝나면 부르는 쪽이 다음 패스의 프레임버퍼와 뷰포트를 다시 잡는다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
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
	/// 프레임버퍼 반환.
	///
	/// # 돌려줌
	/// 깊이 전용 프레임버퍼. (컬러 어태치먼트 없음)
	//==============================================================================
	pub fn get_framebuffer(&self) -> GlFramebuffer {
		self.framebuffer
	}

	//==============================================================================
	/// 깊이 텍스처 반환.
	///
	/// # 돌려줌
	/// 비교 샘플링(sampler2DShadow)용 깊이 텍스처. (DEPTH_COMPONENT24)
	//==============================================================================
	pub fn get_depth_texture(&self) -> GlTexture {
		self.depth_texture
	}

	//==============================================================================
	/// 해상도 반환.
	///
	/// # 돌려줌
	/// 깊이 텍스처 한 변의 텍셀 수.
	//==============================================================================
	pub fn get_resolution(&self) -> i32 {
		self.resolution
	}

	//==============================================================================
	/// 광원 뷰 투영 행렬 반환.
	///
	/// # 돌려줌
	/// 광원 뷰 투영 행렬. (월드 → 광원 클립, update_light_matrix 전에는 단위 행렬)
	//==============================================================================
	pub fn get_light_view_projection_matrix(&self) -> &Matrix4 {
		&self.light_view_projection_matrix
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	///
	/// # 인자
	/// - `gl`: 만들 때 쓴 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_framebuffer(self.framebuffer);
			gl.delete_texture(self.depth_texture);
		}
	}
}

//==============================================================================
/// 반올림. (원본 Math.round 와 같이 x.5 는 위쪽 정수로)
//==============================================================================
fn round_half_up(value: f32) -> f32 {
	(value + 0.5).floor()
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 반올림이 원본 Math.round 와 같은지. (Math.round(-2.5) = -2, Math.round(2.5) = 3)
	//==============================================================================
	#[test]
	fn round_half_up_matches_java_script_round() {
		assert_eq!(round_half_up(2.5), 3.0);
		assert_eq!(round_half_up(-2.5), -2.0);
		assert_eq!(round_half_up(-2.6), -3.0);
		assert_eq!(round_half_up(1.49), 1.0);
	}
}
