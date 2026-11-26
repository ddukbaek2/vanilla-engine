//==============================================================================
//! 섀도우 맵. (원본 graphics3d/shadowmap.js)
//! - 방향광 실시간 그림자, 깊이 텍스처(하드웨어 비교 샘플링, sampler2DShadow) + 직교 광원 행렬.
//! - begin_render() 로 깊이 패스를 시작하고, 결과 텍스처를 본 패스에서 샘플링한다.
//! - 광원 행렬은 텍셀 단위로 스냅해 이동 시 그림자 가장자리 떨림을 줄인다. 빛이 거의 바로 위나 아래로 나아가면 위쪽을 z 축으로 잡아 행렬이 깨지지 않게 한다.
//! - 지을 때 앞의 텍스처와 프레임버퍼 묶음을 돌린다. (모형을 지은 뒤에 지어도 깊이 텍스처가 텍스처 유닛에 남아 그리기가 INVALID_OPERATION 이 되지 않게)
//! - ShadowAtlas: 점광과 스포트라이트의 그림자를 깊이 텍스처 한 장에 칸으로 나눠 담는다. (원본 ShadowAtlas, 유니티 URP 의 추가 빛 그림자 아틀라스 꼴)
//!   칸 크기는 URP 기본과 같이 정한다. 칸마다 tile_resolution 을 바라고, 아틀라스에 다 들어가지 않으면 다 들어갈 때까지 칸 한 변을 반씩 줄인다. (점광 하나면 512, 스포트라이트 하나면 1024)
//!   툰 셰이더는 받는 자리를 노멀 쪽으로 그 거리의 칸 텍셀 (1 + 3 x (1 - dot(N, L))) 개만큼 밀어 견준다. (빛에 비스듬한 면일수록 크게, 그림자 여드름을 막는다)
//! - 텍셀 스냅의 반올림은 원본 Math.round 와 같이 0.5 를 위로 올린다. (러스트 round 는 0 에서 먼 쪽이라 음수에서 다르다)
//! - 원본 해상도 기본값 인자 2048 은 DEFAULT_SHADOWMAP_RESOLUTION 으로 둔다. (러스트에 기본값 인자가 없다)
//==============================================================================
use crate::base::matrix4::Matrix4;
use crate::base::vector3::Vector3;
use crate::core::graphic::GlTexture;
use crate::graphics3d::GlFramebuffer;
use glow::HasContext;

//==============================================================================
// 전역 상수 목록.
//==============================================================================
/// 해상도 기본값. (원본 생성자 기본 인자)
pub const DEFAULT_SHADOWMAP_RESOLUTION: i32 = 2048;

/// 그림자 아틀라스 한 변의 기본값. (원본 ShadowAtlas 의 기본값 인자, 유니티 URP 의 추가 빛 그림자 아틀라스 기본 2048)
pub const DEFAULT_SHADOW_ATLAS_RESOLUTION: i32 = 2048;

/// 그림자 아틀라스 칸마다 바라는 한 변의 기본값. (원본 ShadowAtlas 의 기본값 인자, 유니티 URP 의 추가 빛 그림자 해상도 단계 High 기본 1024, 빛의 기본 단계가 High)
pub const DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION: i32 = 1024;

/// 그림자 아틀라스가 받는 점광과 스포트라이트 수의 상한. (셰이더의 유니폼 배열 크기)
pub const SHADOW_ATLAS_MAXIMUM_LIGHTS: usize = 4;

/// 점광 큐브 여섯 면의 앞쪽과 위쪽. (+x, -x, +y, -y, +z, -z 차례, 오른쪽은 앞쪽 x 위쪽, 셰이더의 면 고르기와 같다)
const CUBE_FACE_FORWARDS: [[f32; 3]; 6] = [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, -1.0]];
const CUBE_FACE_UPS: [[f32; 3]; 6] = [[0.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];

//==============================================================================
/// 깊이 판 짓기. (하드웨어 비교 샘플링 sampler2DShadow 의 DEPTH_COMPONENT24 텍스처와 컬러 없는 프레임버퍼, 앞의 텍스처와 프레임버퍼 묶음은 돌린다)
/// - 깊이 텍스처를 묶은 채로 두면, 그 유닛을 읽는 프로그램이 이 프레임버퍼에 그릴 때 되먹임 고리로 INVALID_OPERATION 이 난다.
//==============================================================================
fn create_depth_target(gl: &glow::Context, resolution: i32) -> Result<(GlFramebuffer, GlTexture), String> {
	unsafe {
		let previous_texture = crate::graphics3d::read_texture_binding(gl);
		let previous_framebuffer = crate::graphics3d::read_framebuffer_binding(gl);
		let depth_texture = gl.create_texture()?;
		gl.bind_texture(glow::TEXTURE_2D, Some(depth_texture));
		gl.tex_image_2d(glow::TEXTURE_2D, 0, glow::DEPTH_COMPONENT24 as i32, resolution, resolution, 0, glow::DEPTH_COMPONENT, glow::UNSIGNED_INT, glow::PixelUnpackData::Slice(None));
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_MODE, glow::COMPARE_REF_TO_TEXTURE as i32);
		gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_COMPARE_FUNC, glow::LEQUAL as i32);
		let framebuffer = gl.create_framebuffer()?;
		gl.bind_framebuffer(glow::FRAMEBUFFER, Some(framebuffer));
		gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::DEPTH_ATTACHMENT, glow::TEXTURE_2D, Some(depth_texture), 0);
		gl.draw_buffers(&[glow::NONE]);
		gl.read_buffer(glow::NONE);
		gl.bind_framebuffer(glow::FRAMEBUFFER, previous_framebuffer);
		gl.bind_texture(glow::TEXTURE_2D, previous_texture);
		Ok((framebuffer, depth_texture))
	}
}

//==============================================================================
/// 앞쪽, 위쪽, 자리로 보기 행렬 짓기. (오른쪽은 앞쪽 x 위쪽, 카메라는 앞쪽을 본다, 셰이더가 같은 바탕으로 칸 자리를 셈한다)
//==============================================================================
fn create_basis_view_matrix(position: [f32; 3], forward: [f32; 3], up: [f32; 3]) -> Matrix4 {
	let right = [forward[1] * up[2] - forward[2] * up[1], forward[2] * up[0] - forward[0] * up[2], forward[0] * up[1] - forward[1] * up[0]];
	let back = [-forward[0], -forward[1], -forward[2]];
	let dot = |axis: [f32; 3]| axis[0] * position[0] + axis[1] * position[1] + axis[2] * position[2];
	let mut matrix = Matrix4::create_identity();
	let elements = &mut matrix.elements;
	elements[0] = right[0];
	elements[4] = right[1];
	elements[8] = right[2];
	elements[12] = -dot(right);
	elements[1] = up[0];
	elements[5] = up[1];
	elements[9] = up[2];
	elements[13] = -dot(up);
	elements[2] = back[0];
	elements[6] = back[1];
	elements[10] = back[2];
	elements[14] = -dot(back);
	elements[3] = 0.0;
	elements[7] = 0.0;
	elements[11] = 0.0;
	elements[15] = 1.0;
	matrix
}

//==============================================================================
/// 3 수 벡터 길이를 1 로. (길이 0 이면 아래쪽)
//==============================================================================
fn normalize_direction(direction: [f32; 3]) -> [f32; 3] {
	let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
	if length <= 0.0 {
		return [0.0, -1.0, 0.0];
	}
	[direction[0] / length, direction[1] / length, direction[2] / length]
}

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
	/// 생성. (깊이 텍스처 + 컬러 없는 프레임버퍼, 앞의 텍스처와 프레임버퍼 묶음은 돌린다)
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
		let (framebuffer, depth_texture) = create_depth_target(gl, resolution)?;
		Ok(ShadowMap { framebuffer, depth_texture, resolution, light_view_projection_matrix: Matrix4::create_identity() })
	}

	//==============================================================================
	/// 광원 행렬 갱신. (초점 주변을 덮는 직교 투영, 텍셀 스냅, 빛이 거의 바로 위나 아래로 나아가면 위쪽을 z 축으로)
	/// - light_direction 은 빛이 나아가는 방향, extent 는 초점 기준 절반 커버 범위다.
	/// - 광원 눈은 초점에서 빛 반대쪽으로 (near_distance + far_distance) / 2 만큼 떨어지고, 위쪽은 Vector3::up() 이다.
	///   빛 방향의 y 성분 절댓값이 0.99 를 넘으면(거의 바로 위나 아래로) 위쪽을 z 축(0, 0, 1)으로 바꾼다.
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
		let up_direction = read_light_up_direction(normalized_direction);
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
/// 그림자 아틀라스의 빛 하나. (셰이더에 올릴 값)
///
/// # 예제
/// 아틀라스가 고른 빛과 그 첫 칸 읽기
/// ```rust,no_run
/// use vanilla::ShadowAtlas;
///
/// fn print_shadow_lights(shadow_atlas: &ShadowAtlas) {
/// 	for light in shadow_atlas.get_lights() {
/// 		let tile_count = if light.tangent > 0.0 {
/// 			1
/// 		}
/// 		else {
/// 			6
/// 		};
/// 		println!("light {} uses tiles {}..{}", light.light_index, light.first_tile, light.first_tile + tile_count);
/// 	}
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowAtlasLight {
	/// 빛 목록에서의 차례. (ToonRenderer 가 그 점광에 이 그림자를 붙인다)
	pub light_index: usize,
	/// 빛의 자리.
	pub position: [f32; 3],
	/// 보는 쪽. (스포트라이트만)
	pub forward: [f32; 3],
	/// 위쪽. (스포트라이트만)
	pub up: [f32; 3],
	/// 반각의 탄젠트. (0 이면 점광)
	pub tangent: f32,
	/// 가까운 거리.
	pub near: f32,
	/// 먼 거리.
	pub far: f32,
	/// 첫 칸 번호.
	pub first_tile: usize,
}

//==============================================================================
/// 그림자 아틀라스의 칸 하나.
///
/// # 예제
/// 칸마다 뷰포트 자리 읽기
/// ```rust,no_run
/// use vanilla::ShadowAtlas;
///
/// fn print_tiles(shadow_atlas: &ShadowAtlas) {
/// 	for tile_index in 0..shadow_atlas.get_tile_count() {
/// 		let tile = shadow_atlas.get_tile(tile_index);
/// 		println!("tile {} at ({}, {}) size {}", tile_index, tile.x, tile.y, tile.size);
/// 	}
/// }
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug)]
pub struct ShadowAtlasTile {
	/// 칸의 보기와 투영.
	pub view_projection_matrix: Matrix4,
	/// 칸의 왼쪽 아래 x. (픽셀)
	pub x: i32,
	/// 칸의 왼쪽 아래 y. (픽셀)
	pub y: i32,
	/// 칸 한 변. (픽셀)
	pub size: i32,
}

//==============================================================================
/// 그림자 아틀라스가 받는 빛 꼴. (원본 updateLights 의 객체 하나, ToonPointLight 에서 짓는다)
///
/// # 예제
/// 점광 하나와 스포트라이트 하나
/// ```rust
/// use vanilla::ShadowAtlasLightSource;
///
/// let point_light = ShadowAtlasLightSource {
/// 	position: [0.0, 2.0, 0.0],
/// 	direction: None,
/// 	outer_cone_angle: 0.0,
/// 	range: 10.0,
/// 	shadow_near: 0.05,
/// 	casts_shadow: true,
/// };
/// let spot_light = ShadowAtlasLightSource {
/// 	position: [3.0, 4.0, 0.0],
/// 	direction: Some([0.0, -1.0, 0.0]),
/// 	outer_cone_angle: std::f32::consts::FRAC_PI_4,
/// 	..point_light
/// };
/// assert!(point_light.direction.is_none());
/// assert!(spot_light.direction.is_some());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowAtlasLightSource {
	/// 빛의 자리.
	pub position: [f32; 3],
	/// 빛이 나아가는 쪽. (Some 이면 스포트라이트)
	pub direction: Option<[f32; 3]>,
	/// 스포트라이트 바깥 원뿔 반각. (라디안)
	pub outer_cone_angle: f32,
	/// 먼 거리. (0 이면 30)
	pub range: f32,
	/// 가까운 거리.
	pub shadow_near: f32,
	/// 그림자를 드리울지.
	pub casts_shadow: bool,
}

//==============================================================================
/// 그림자 아틀라스. (점광과 스포트라이트의 실시간 그림자를 깊이 텍스처 한 장에 칸으로 나눠 담는다, 원본 ShadowAtlas)
/// - 스포트라이트는 원근 그림자 맵 한 칸, 점광은 큐브 여섯 면을 칸 여섯에 (면마다 90 도 원근) 그린다.
/// - update_lights(빛 목록) 이 casts_shadow 가 true 인 빛을 SHADOW_ATLAS_MAXIMUM_LIGHTS 까지 골라 칸을 나누고 칸마다 보기와 투영을 셈한다.
/// - 칸은 한 변 grid_size 의 격자다. (grid_size = 올림(칸 수의 제곱근), 칸 한 변 = 내림(해상도 / grid_size))
/// - 그리기: begin_render 로 판을 비우고, 칸마다 begin_tile 로 뷰포트를 건 뒤 그 칸의 보기와 투영으로 깊이를 그린다. (ToonRenderer::begin_shadow_atlas_pass, set_shadow_atlas_tile)
///
/// # 예제
/// 빛 목록으로 칸을 나누고 칸마다 깊이 그리기
/// ```rust,no_run
/// use vanilla::{ShadowAtlas, ShadowAtlasLightSource};
///
/// fn render_shadows(gl: &glow::Context, shadow_atlas: &mut ShadowAtlas, light_source_list: &[ShadowAtlasLightSource]) {
/// 	shadow_atlas.update_lights(light_source_list);
/// 	shadow_atlas.begin_render(gl);
/// 	for tile_index in 0..shadow_atlas.get_tile_count() {
/// 		shadow_atlas.begin_tile(gl, tile_index);
/// 		let _view_projection_matrix = shadow_atlas.get_tile(tile_index).view_projection_matrix;
/// 	}
/// }
/// ```
//==============================================================================
pub struct ShadowAtlas {
	framebuffer: GlFramebuffer,
	depth_texture: GlTexture,
	resolution: i32,
	/// 칸마다 바라는 한 변. (픽셀)
	tile_resolution: i32,
	grid_size: i32,
	tile_size: i32,
	light_list: Vec<ShadowAtlasLight>,
	tile_list: Vec<ShadowAtlasTile>,
}

impl ShadowAtlas {
	//==============================================================================
	/// 생성. (깊이 텍스처와 컬러 없는 프레임버퍼, 앞의 묶음은 돌린다)
	/// - 칸이 없는 상태로 시작한다. (격자 한 변 1, 칸 한 변은 해상도, update_lights 로 나눈다)
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `resolution`: 깊이 텍스처 한 변의 텍셀 수. (정사각형)
	///
	/// # 돌려줌
	/// 새 그림자 아틀라스. 텍스처나 프레임버퍼를 못 만들면 Err(GL 의 오류 글).
	//==============================================================================
	/// - resolution 은 아틀라스 한 변, tile_resolution 은 칸마다 바라는 한 변이다. (원본 기본값은 DEFAULT_SHADOW_ATLAS_RESOLUTION, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION)
	pub fn new(gl: &glow::Context, resolution: i32, tile_resolution: i32) -> Result<ShadowAtlas, String> {
		let (framebuffer, depth_texture) = create_depth_target(gl, resolution)?;
		Ok(ShadowAtlas { framebuffer, depth_texture, resolution, tile_resolution: tile_resolution.min(resolution).max(1), grid_size: 1, tile_size: resolution, light_list: Vec::new(), tile_list: Vec::new() })
	}

	//==============================================================================
	/// 빛 목록으로 칸 나누기. (casts_shadow 가 true 인 것을 SHADOW_ATLAS_MAXIMUM_LIGHTS 까지, 스포트라이트 한 칸, 점광 여섯 칸)
	/// - 가까운 거리는 0.001 아래로, 먼 거리는 가까운 거리 + 0.01 아래로 내려가지 않는다. (range 가 0 이하면 30)
	/// - 스포트라이트의 원근 반각은 바깥 원뿔 반각(0.01 이상)에 0.05 라디안을 더하고 1.45 라디안에서 자른다.
	///
	/// # 인자
	/// - `light_source_list`: 빛 목록. (차례가 ShadowAtlasLight::light_index 가 된다, ToonLightSet::read_shadow_light_sources 로 짓는다)
	//==============================================================================
	pub fn update_lights(&mut self, light_source_list: &[ShadowAtlasLightSource]) {
		self.light_list.clear();
		let mut tile_count = 0usize;
		for (light_index, light_source) in light_source_list.iter().enumerate() {
			if !light_source.casts_shadow || self.light_list.len() >= SHADOW_ATLAS_MAXIMUM_LIGHTS {
				continue;
			}
			let near = light_source.shadow_near.max(0.001);
			let far = (if light_source.range > 0.0 { light_source.range } else { 30.0 }).max(near + 0.01);
			let mut light = ShadowAtlasLight { light_index, position: light_source.position, forward: [0.0, 0.0, 1.0], up: [0.0, 1.0, 0.0], tangent: 0.0, near, far, first_tile: tile_count };
			match light_source.direction {
				Some(direction) => {
					let direction = normalize_direction(direction);
					let helper = if direction[1].abs() < 0.99 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
					let along = helper[0] * direction[0] + helper[1] * direction[1] + helper[2] * direction[2];
					light.forward = direction;
					light.up = normalize_direction([helper[0] - direction[0] * along, helper[1] - direction[1] * along, helper[2] - direction[2] * along]);
					light.tangent = (light_source.outer_cone_angle.max(0.01) + 0.05).min(1.45).tan();
					tile_count += 1;
				}
				None => {
					tile_count += 6;
				}
			}
			self.light_list.push(light);
		}
		self.tile_size = read_shadow_atlas_tile_size(self.resolution, self.tile_resolution, tile_count);
		self.grid_size = (self.resolution / self.tile_size).max(1);
		self.tile_list.clear();
		for light_position in 0..self.light_list.len() {
			let light = self.light_list[light_position];
			if light.tangent > 0.0 {
				let tile = self.create_tile(light.position, light.forward, light.up, light.tangent, light.near, light.far);
				self.tile_list.push(tile);
				continue;
			}
			for face_index in 0..6 {
				let tile = self.create_tile(light.position, CUBE_FACE_FORWARDS[face_index], CUBE_FACE_UPS[face_index], 1.0, light.near, light.far);
				self.tile_list.push(tile);
			}
		}
	}

	//==============================================================================
	/// 칸 하나 짓기. (지금까지 지은 칸 수로 격자 자리를 정하고, 바탕 보기 행렬과 원근 투영을 곱한다)
	//==============================================================================
	fn create_tile(&self, position: [f32; 3], forward: [f32; 3], up: [f32; 3], tangent: f32, near: f32, far: f32) -> ShadowAtlasTile {
		let tile_index = self.tile_list.len() as i32;
		let mut view_projection_matrix = Matrix4::create_perspective(2.0 * tangent.atan(), 1.0, near, far);
		view_projection_matrix.multiply(&create_basis_view_matrix(position, forward, up));
		ShadowAtlasTile { view_projection_matrix, x: (tile_index % self.grid_size) * self.tile_size, y: (tile_index / self.grid_size) * self.tile_size, size: self.tile_size }
	}

	//==============================================================================
	/// 깊이 패스 시작. (프레임버퍼를 묶고 판 전체를 비운다, 칸마다 begin_tile)
	/// - 뷰포트를 판 전체로, 가위 시험을 끄고, 깊이 시험과 깊이 쓰기를 켠다.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	//==============================================================================
	pub fn begin_render(&self, gl: &glow::Context) {
		unsafe {
			gl.bind_framebuffer(glow::FRAMEBUFFER, Some(self.framebuffer));
			gl.viewport(0, 0, self.resolution, self.resolution);
			gl.disable(glow::SCISSOR_TEST);
			gl.enable(glow::DEPTH_TEST);
			gl.depth_mask(true);
			gl.clear(glow::DEPTH_BUFFER_BIT);
		}
	}

	//==============================================================================
	/// 칸 하나에 뷰포트 걸기.
	///
	/// # 인자
	/// - `gl`: GL 컨텍스트.
	/// - `tile_index`: 칸 번호. (0 ~ get_tile_count() - 1, 밖이면 패닉)
	//==============================================================================
	pub fn begin_tile(&self, gl: &glow::Context, tile_index: usize) {
		let tile = self.tile_list[tile_index];
		unsafe {
			gl.viewport(tile.x, tile.y, tile.size, tile.size);
		}
	}

	//==============================================================================
	/// 그림자를 받는 빛 반환.
	///
	/// # 돌려줌
	/// 마지막 update_lights 가 고른 빛 목록. (SHADOW_ATLAS_MAXIMUM_LIGHTS 개까지)
	//==============================================================================
	pub fn get_lights(&self) -> &[ShadowAtlasLight] {
		&self.light_list
	}

	//==============================================================================
	/// 칸마다 바라는 한 변 반환. (픽셀, 생성 때 준 값)
	//==============================================================================
	pub fn get_tile_resolution(&self) -> i32 {
		self.tile_resolution
	}

	//==============================================================================
	/// 칸 수 반환.
	///
	/// # 돌려줌
	/// 칸 수. (스포트라이트 1, 점광 6 씩 더한 것)
	//==============================================================================
	pub fn get_tile_count(&self) -> usize {
		self.tile_list.len()
	}

	//==============================================================================
	/// 칸 반환.
	///
	/// # 인자
	/// - `tile_index`: 칸 번호. (0 ~ get_tile_count() - 1, 밖이면 패닉)
	///
	/// # 돌려줌
	/// 그 칸의 보기와 투영, 자리, 크기.
	//==============================================================================
	pub fn get_tile(&self, tile_index: usize) -> &ShadowAtlasTile {
		&self.tile_list[tile_index]
	}

	//==============================================================================
	/// 격자 한 변의 칸 수 반환.
	///
	/// # 돌려줌
	/// 격자 한 변의 칸 수. (올림(칸 수의 제곱근), 1 이상)
	//==============================================================================
	pub fn get_grid_size(&self) -> i32 {
		self.grid_size
	}

	//==============================================================================
	/// 칸 한 변 반환. (픽셀)
	///
	/// # 돌려줌
	/// 칸 한 변의 픽셀 수. (내림(해상도 / 격자 한 변))
	//==============================================================================
	pub fn get_tile_size(&self) -> i32 {
		self.tile_size
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
	/// 깊이 텍스처 반환. (비교 샘플링 sampler2DShadow)
	///
	/// # 돌려줌
	/// 아틀라스 깊이 텍스처의 GL 손잡이.
	//==============================================================================
	pub fn get_depth_texture(&self) -> GlTexture {
		self.depth_texture
	}

	//==============================================================================
	/// 프레임버퍼 반환.
	///
	/// # 돌려줌
	/// 깊이만 붙은 프레임버퍼의 GL 손잡이.
	//==============================================================================
	pub fn get_framebuffer(&self) -> GlFramebuffer {
		self.framebuffer
	}

	//==============================================================================
	/// 파괴. (GL 리소스 해제)
	///
	/// # 인자
	/// - `gl`: 이 아틀라스를 지은 GL 컨텍스트.
	//==============================================================================
	pub fn destroy(&self, gl: &glow::Context) {
		unsafe {
			gl.delete_framebuffer(self.framebuffer);
			gl.delete_texture(self.depth_texture);
		}
	}
}

//==============================================================================
/// 아틀라스 칸 한 변 읽기. (칸마다 바라는 한 변에서 시작해, 칸이 다 들어가지 않으면 다 들어갈 때까지 반씩 줄인다, 유니티 URP 의 shadowSlicesScaleFactor 를 두 배씩)
//==============================================================================
fn read_shadow_atlas_tile_size(resolution: i32, tile_resolution: i32, tile_count: usize) -> i32 {
	let mut tile_size = tile_resolution.min(resolution).max(1);
	while tile_size > 1 && tile_count as i32 > (resolution / tile_size) * (resolution / tile_size) {
		tile_size /= 2;
	}
	tile_size
}

//==============================================================================
/// 광원 보기의 위쪽 읽기. (빛이 거의 바로 위나 아래로 나아가면 위쪽이 빛과 나란해 보기 행렬이 깨지므로 z 축)
//==============================================================================
fn read_light_up_direction(normalized_direction: Vector3) -> Vector3 {
	if normalized_direction.y.abs() > 0.99 {
		Vector3::create(0.0, 0.0, 1.0)
	}
	else {
		Vector3::up()
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
	/// 아틀라스 칸 한 변과 큐브 면 보기. (URP 기본처럼 1024 에서 반씩, 칸 7 개면 512, 보기 행렬이 앞쪽을 -z 로)
	//==============================================================================
	#[test]
	fn shadow_atlas_tiles_cover_lights() {
		assert_eq!(read_shadow_atlas_tile_size(2048, 1024, 1), 1024);
		assert_eq!(read_shadow_atlas_tile_size(2048, 1024, 4), 1024);
		assert_eq!(read_shadow_atlas_tile_size(2048, 1024, 6), 512);
		assert_eq!(read_shadow_atlas_tile_size(2048, 1024, 7), 512);
		assert_eq!(read_shadow_atlas_tile_size(2048, 1024, 24), 256);
		assert_eq!(read_shadow_atlas_tile_size(1024, 4096, 1), 1024);
		let view_matrix = create_basis_view_matrix([1.0, 2.0, 3.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]);
		let point = view_matrix.transform_point(2.0, 2.0, 3.0);
		assert!((point.z + 1.0).abs() < 0.0001);
		assert!(point.x.abs() < 0.0001 && point.y.abs() < 0.0001);
	}

	//==============================================================================
	/// 빛이 바로 아래를 볼 때 광원 보기가 깨지지 않는지. (위쪽이 빛과 나란하면 NaN)
	//==============================================================================
	#[test]
	fn vertical_light_keeps_view_matrix() {
		let direction = Vector3::create(0.0, -1.0, 0.0).normalize();
		let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 10.0, 0.0), Vector3::create(0.0, 0.0, 0.0), read_light_up_direction(direction));
		assert!(view_matrix.elements.iter().all(|value| value.is_finite()));
		let point = view_matrix.transform_point(0.0, 0.0, 0.0);
		assert!((point.z + 10.0).abs() < 0.0001);
		assert_eq!(read_light_up_direction(Vector3::create(0.6, -0.8, 0.0)).y, 1.0);
	}

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
