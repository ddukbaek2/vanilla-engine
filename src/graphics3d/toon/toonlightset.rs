//==============================================================================
//! 툰 빛 묶음. (vanilla.js graphics3d/toon/toonlightset.js 의 ToonLightSet)
//! - ToonRenderer 가 몸을 칠할 빛과 툰 빛 반응을 한 덩어리로 든다. 렌더러는 기본 묶음 하나를 들고, set_light 같은 렌더러의 빛 설정은 그 묶음을 고친다.
//!   그리기마다 ToonDrawOptions 의 light_set 으로 다른 묶음을 줄 수 있다. (재질마다 그늘 색이 다를 때)
//! - 해(주광): 방향(sun_direction)은 빛이 나아가는 쪽이다. 면에서 해 쪽을 가리키는 방향의 반대다. (위에서 내리쬐면 [0, -1, 0]) 그림자를 받는다.
//! - 바탕빛: 하늘색과 땅색을 노멀의 위아래(N.y x 0.5 + 0.5)로 섞는다. (반구 빛)
//! - 그늘 램프(셀 셰이딩): 빛을 받는 몫 x 를 램프에 대어 색을 고른다. x 는 반 램버트면 dot(N, L) x 0.5 + 0.5, 아니면 max(dot(N, L), 0) 이고,
//!   그리기의 shade_offset 을 더한 뒤 해는 그림자(받는 몫 0 ~ 1)를 곱한다. (그림자가 검정이 아니라 그늘 색으로 들어간다, 유니티 툰 셰이더의 System Shadows)
//!   램프는 칸(stop)마다 자리, 색(선형 배), 부드러움이다. 칸 i 의 색은 x 가 (자리 - 부드러움) 에서 자리까지 오를 때 앞 칸 색에서 넘어간다. (유니티 툰 셰이더의 Step 과 Feather)
//!   부드러움이 화소 하나 너비(fwidth)보다 작으면 그 너비로 막아 경계가 계단지지 않는다.
//!   set_shade 는 바탕, 1 차 그늘, 2 차 그늘 셋으로, set_shade_ramp 는 칸을 바로, set_band_count 는 고른 칸으로, set_band_ramp 는 회색 칸으로 램프를 짓는다.
//!   set_ramp_texture 를 주면 칸 대신 그 텍스처의 가로(x 를 u 로, v 는 0.5)를 읽는다. 칸이 없으면(clear_shade_ramp, set_band_count(0)) 부드러운 램버트다.
//! - 빛마다: 해는 램프 색을 통째로 받아 그늘도 해의 색으로 물든다. 채움 빛, 점광, 스포트라이트는 램프의 밝기 단계(바닥 칸이 0, 맨 위 칸이 1)를 받아 등진 면을 밝히지 않는다.
//! - 채움 빛(fill): 그림자를 받지 않는 둘째 방향광. 방향의 뜻은 해와 같다. 기본 색 0 이면 없다.
//! - 하이라이트(정반사): 블린 퐁 pow(max(dot(N, H), 0), exponent) 를 0.5 에서 가른 단단한 점(부드러움 0)에서 그대로의 부드러운 빛(부드러움 1)까지. 색 x 세기 x 빛 색 x 빛을 받는 단계.
//!   해와 점광, 스포트라이트 모두에 선다. 기본 세기 0 이면 없다.
//! - 림 빛(rim): pow(1 - max(dot(N, V), 0), power) 를 0.5 에서 가른 단단한 테(부드러움 0)에서 부드러운 테(1)까지. light_mask 가 1 이면 해를 받는 쪽에만. 바탕색과 상관없이 더한다.
//!   그늘 쪽 림(set_shade_rim_light)은 같은 테를 해와 등진 쪽에 (테 - (dot(N, L) x 0.5 + 0.5)) 만큼 더한다. (유니티 툰 셰이더의 Antipodean(Ap) Rim Light)
//! - 빛 색 쓰기(set_light_color_usage): 색마다 빛 색을 곱할지 고른다. 끄면 바탕, 그늘, 하이라이트는 빛의 세기(0.299 R + 0.587 G + 0.114 B)만, 림과 매트캡과 천사 고리와 바깥선은 빛을 받지 않는다. (유니티 툰 셰이더의 Is_LightColor_*, 기본 모두 켬)
//! - 빛 세기 막기(set_light_intensity_limited): 빛마다 받은 몫을 1 로 막는다. 해와 채움 빛은 채널마다 1 로 자르고, 점광과 스포트라이트는 감쇠까지 곱한 색의 세기가 1 을 넘으면 세기로 나눈다.
//!   (유니티 툰 셰이더의 Limit Light Intensity, 기본 켬) HDR 블룸에 센 빛을 쓰려면 끈다.
//!   옛 게임 셰이더(빛 세기를 막지 않던 것)를 옮길 때나 세기 1 을 넘는 점광, HDR 블룸을 쓸 때는 set_light_intensity_limited(false).
//! - 점광과 스포트라이트: TOON_MAXIMUM_POINT_LIGHTS 까지. (ToonPointLight)
//!   감쇠는 창 x 거리 감쇠다. 창은 range 가 0 보다 크면 (1 - (거리 / range)^range_exponent) 를 0 ~ 1 로 막아 제곱, 0 이면 1. 거리 감쇠는 decay 가 0 보다 크면 1 / max(거리^decay, 0.01), 0 이면 1.
//!   casts_shadow 가 true 인 빛은 ShadowAtlas::update_lights(read_shadow_light_sources()) 가 그림자 칸을 준다. (점광은 큐브 여섯 면, 스포트라이트는 원근 한 칸)
//!   direction(빛이 나아가는 쪽)이 있으면 스포트라이트다. 각 감쇠는 glTF KHR_lights_punctual 꼴로 clamp(cos 각 x scale + offset, 0, 1) 의 제곱이다.
//!   받는 몫은 감싸기(wrap) (dot(N, L) + wrap) / (1 + wrap) 이다. (0 이면 램버트, GPU Gems 1 의 wrap lighting)
//! - 빛 꼴: set_light_model(ToonLightModel { is_energy_normalized }) 가 true 면 바탕색 x 빛을 π 로 나누고 하이라이트에 (exponent + 8) / 8π 를 곱한다.
//! - 색은 모두 선형 색이다. (HDR 이면 1 을 넘어도 된다) 램프 텍스처는 sRGB 로 보고 선형으로 바꿔 읽는다.
//! - 고치면 판 번호(get_version)가 오른다. 렌더러는 묶음 번호(get_id)와 판 번호가 같으면 다시 올리지 않는다.
//!
//! # 예제
//! 그늘 색 두 단계와 단단한 하이라이트, 림, 스포트라이트를 건 빛 묶음
//! ```rust
//! use vanilla::{ToonLightSet, ToonPointLight, ToonShade};
//!
//! let mut light_set = ToonLightSet::new();
//! light_set.set_light([-0.4, -1.0, -0.3], [0.85, 0.82, 0.78], [0.3, 0.33, 0.42], [0.16, 0.15, 0.14]);
//! light_set.set_shade(ToonShade { step: 0.5, feather: 0.02, color: [0.55, 0.52, 0.7], second_step: 0.25, second_feather: 0.02, second_color: [0.36, 0.33, 0.52], is_half_lambert: true });
//! light_set.set_specular(0.6, 64.0, 0.0, [1.0, 1.0, 1.0]);
//! light_set.set_rim_light([0.5, 0.55, 0.7], 3.0, 0.2, 1.0);
//! light_set.set_point_lights(&[ToonPointLight { position: [0.0, 3.0, 0.0], color: [1.0, 0.8, 0.6], intensity: 3.0, range: 6.0, direction: Some([0.0, -1.0, 0.0]), inner_cone_angle: 0.3, outer_cone_angle: 0.6, ..ToonPointLight::default() }]);
//! assert_eq!(light_set.get_shade_ramp().positions.len(), 3);
//! assert_eq!(light_set.get_point_light_count(), 1);
//! ```
//==============================================================================
use std::sync::atomic::{AtomicU32, Ordering};

use crate::core::graphic::GlTexture;
use crate::graphics3d::shadowmap::ShadowAtlasLightSource;

/// 한 그리기에 받는 점광과 스포트라이트 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_POINT_LIGHTS: usize = 8;

/// 그늘 램프 칸 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_BAND_STEPS: usize = 8;

/// 빛 묶음마다 다른 번호를 주는 셈. (0 은 쓰지 않는다)
static NEXT_LIGHT_SET_ID: AtomicU32 = AtomicU32::new(1);

//==============================================================================
/// 점광이나 스포트라이트 하나. (원본 setPointLights 의 객체 하나)
///
/// # 예제
/// 아래를 비추는 스포트라이트 하나
/// ```rust
/// use vanilla::{ToonLightSet, ToonPointLight};
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_point_lights(&[ToonPointLight { position: [0.0, 4.0, 0.0], color: [1.0, 0.9, 0.7], intensity: 4.0, range: 8.0, direction: Some([0.0, -1.0, 0.0]), inner_cone_angle: 0.25, outer_cone_angle: 0.5, ..ToonPointLight::default() }]);
/// assert_eq!(light_set.get_point_lights()[0].direction, Some([0.0, -1.0, 0.0]));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonPointLight {
	/// 월드 자리.
	pub position: [f64; 3],
	/// 선형 색.
	pub color: [f64; 3],
	/// 색에 곱할 세기.
	pub intensity: f64,
	/// 창의 범위. (0 이면 끝없음)
	pub range: f64,
	/// 거리 감쇠 지수. (0 이면 거리 감쇠 없음)
	pub decay: f64,
	/// 창 꼴 지수.
	pub range_exponent: f64,
	/// 감싸기 몫. (0 이면 램버트)
	pub wrap: f64,
	/// 빛이 나아가는 쪽. (Some 이면 스포트라이트)
	pub direction: Option<[f64; 3]>,
	/// 스포트라이트 안쪽 원뿔 반각. (라디안, 여기까지 다 밝다)
	pub inner_cone_angle: f64,
	/// 스포트라이트 바깥 원뿔 반각. (라디안, 여기서 0 이 된다)
	pub outer_cone_angle: f64,
	/// 그림자를 드리울지. (ShadowAtlas 가 칸을 준다)
	pub casts_shadow: bool,
	/// 그림자의 가까운 거리. (먼 거리는 range, 0 이면 30)
	pub shadow_near: f64,
}

impl Default for ToonPointLight {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonPointLight { position: [0.0; 3], color: [1.0; 3], intensity: 1.0, range: 0.0, decay: 2.0, range_exponent: 4.0, wrap: 0.0, direction: None, inner_cone_angle: 0.0, outer_cone_angle: std::f64::consts::FRAC_PI_4, casts_shadow: false, shadow_near: 0.05 }
	}
}

//==============================================================================
/// 색마다 빛 색을 곱할지. (유니티 툰 셰이더의 Is_LightColor_*, 기본 모두 true)
/// - 바탕, 그늘, 하이라이트는 끄면 빛의 휘도(회색)를 받고, 나머지는 끄면 빛을 받지 않는다.
///
/// # 예제
/// 그늘과 바깥선은 빛 색에 물들지 않게
/// ```rust
/// use vanilla::{ToonLightColorUsage, ToonLightSet};
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_light_color_usage(ToonLightColorUsage { shade: false, outline: false, ..ToonLightColorUsage::default() });
/// assert!(light_set.get_light_color_usage().base);
/// assert!(!light_set.get_light_color_usage().shade);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ToonLightColorUsage {
	/// 바탕.
	pub base: bool,
	/// 그늘.
	pub shade: bool,
	/// 하이라이트.
	pub highlight: bool,
	/// 림.
	pub rim: bool,
	/// 그늘 쪽 림.
	pub shade_rim: bool,
	/// 매트캡.
	pub matcap: bool,
	/// 천사 고리.
	pub angel_ring: bool,
	/// 껍데기 바깥선.
	pub outline: bool,
}

impl Default for ToonLightColorUsage {
	//==============================================================================
	/// 기본값. (모두 켬)
	//==============================================================================
	fn default() -> Self {
		ToonLightColorUsage { base: true, shade: true, highlight: true, rim: true, shade_rim: true, matcap: true, angel_ring: true, outline: true }
	}
}

//==============================================================================
/// 그늘 쪽 림 빛. (선형 색과 거듭제곱, 거듭제곱이 0 이하면 없음)
///
/// # 예제
/// 그늘 쪽에 푸른 테
/// ```rust
/// use vanilla::ToonLightSet;
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_rim_light([0.8, 0.75, 0.6], 3.0, 0.1, 1.0);
/// light_set.set_shade_rim_light([0.2, 0.3, 0.6], 2.5);
/// assert_eq!(light_set.get_shade_rim_light().power, 2.5);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonShadeRimLight {
	/// 더할 선형 색. ([r, g, b])
	pub color: [f64; 3],
	/// 거듭제곱. (0 이하면 없음)
	pub power: f64,
}

//==============================================================================
/// 빛 꼴. (is_energy_normalized 바탕색 x 빛을 π 로 나눌지)
///
/// # 예제
/// 빛 세기를 조도로 주는 빛 꼴
/// ```rust
/// use vanilla::{ToonLightModel, ToonLightSet};
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_light_model(ToonLightModel { is_energy_normalized: true });
/// assert!(light_set.get_light_model().is_energy_normalized);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToonLightModel {
	/// 바탕색 x 빛을 π 로 나눌지. (하이라이트에는 (exponent + 8) / 8π 를 곱한다, 기본 false)
	pub is_energy_normalized: bool,
}

//==============================================================================
/// 그늘 셋. (유니티 툰 셰이더의 바탕, 1 차 그늘, 2 차 그늘과 그 경계의 Step, Feather, second_step 이 0 이하면 2 차 그늘 없음)
///
/// # 예제
/// 따뜻한 1 차 그늘과 짙은 2 차 그늘
/// ```rust
/// use vanilla::{ToonLightSet, ToonShade};
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_shade(ToonShade { step: 0.5, feather: 0.03, color: [0.8, 0.62, 0.6], second_step: 0.2, second_feather: 0.03, second_color: [0.5, 0.36, 0.42], is_half_lambert: true });
/// assert_eq!(light_set.get_shade_ramp().positions, vec![0.0, 0.2, 0.5]);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonShade {
	/// 바탕과 1 차 그늘의 경계. (빛을 받는 몫 x 의 자리, 반 램버트면 0.5 가 해와 직각인 면)
	pub step: f64,
	/// 바탕과 1 차 그늘 경계의 부드러움.
	pub feather: f64,
	/// 1 차 그늘 색. (바탕색과 빛에 곱하는 선형 배)
	pub color: [f64; 3],
	/// 1 차와 2 차 그늘의 경계. (0 이하면 2 차 그늘 없음)
	pub second_step: f64,
	/// 1 차와 2 차 그늘 경계의 부드러움.
	pub second_feather: f64,
	/// 2 차 그늘 색.
	pub second_color: [f64; 3],
	/// x 를 반 램버트로 셀지.
	pub is_half_lambert: bool,
}

impl Default for ToonShade {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonShade { step: 0.5, feather: 0.02, color: [0.5, 0.48, 0.66], second_step: 0.0, second_feather: 0.02, second_color: [0.32, 0.3, 0.48], is_half_lambert: true }
	}
}

//==============================================================================
/// 그늘 램프. (칸마다 자리, 선형 색, 부드러움, 칸이 비면 부드러운 램버트, 첫 칸의 자리와 부드러움은 쓰지 않는다)
///
/// # 예제
/// 세 칸 램프를 바로 주기
/// ```rust
/// use vanilla::ToonLightSet;
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_shade_ramp(&[0.0, 0.4, 0.62], &[[0.3, 0.3, 0.45], [0.62, 0.6, 0.75], [1.0, 1.0, 1.0]], &[0.0, 0.05, 0.02], true);
/// let shade_ramp = light_set.get_shade_ramp();
/// assert_eq!(shade_ramp.positions, vec![0.0, 0.4, 0.62]);
/// assert!(shade_ramp.is_half_lambert);
/// ```
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToonShadeRamp {
	/// 칸마다의 자리. (빛을 받는 몫 x, 오름차순)
	pub positions: Vec<f64>,
	/// 칸마다의 선형 [r, g, b] 배.
	pub colors: Vec<[f64; 3]>,
	/// 칸마다의 부드러움. (그 칸으로 넘어가는 너비)
	pub feathers: Vec<f64>,
	/// x 를 dot x 0.5 + 0.5 로 셀지. (아니면 max(dot, 0))
	pub is_half_lambert: bool,
}

//==============================================================================
/// 림 빛. (선형 색, 거듭제곱, 부드러움 0 ~ 1, 해를 받는 쪽에만 둘 몫 0 ~ 1)
///
/// # 예제
/// 해 쪽에만 서는 단단한 테
/// ```rust
/// use vanilla::ToonLightSet;
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_rim_light([0.6, 0.65, 0.8], 2.5, 0.0, 1.0);
/// let rim_light = light_set.get_rim_light();
/// assert_eq!((rim_light.feather, rim_light.light_mask), (0.0, 1.0));
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonRimLight {
	/// 더할 선형 색. ([r, g, b])
	pub color: [f64; 3],
	/// 거듭제곱. (클수록 테가 가늘다, 0 이면 없음)
	pub power: f64,
	/// 0 이면 단단한 테, 1 이면 부드러운 테.
	pub feather: f64,
	/// 1 이면 해를 받는 쪽에만 테를 둔다. (0 ~ 1)
	pub light_mask: f64,
}

//==============================================================================
/// 하이라이트. (세기, 블린 퐁 지수, 부드러움 0 ~ 1, 선형 색)
///
/// # 예제
/// 부드러운 하이라이트
/// ```rust
/// use vanilla::ToonLightSet;
///
/// let mut light_set = ToonLightSet::new();
/// light_set.set_specular(0.4, 32.0, 1.0, [1.0, 0.95, 0.9]);
/// assert_eq!(light_set.get_specular().exponent, 32.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonSpecular {
	/// 세기. (0 이면 없음)
	pub strength: f64,
	/// 블린 퐁 지수. (클수록 작은 점, 1 밑으로 내리지 않는다)
	pub exponent: f64,
	/// 부드러움. (0 이면 단단한 툰 점, 1 이면 부드러운 블린 퐁)
	pub feather: f64,
	/// 선형 [r, g, b].
	pub color: [f64; 3],
}

//==============================================================================
/// 3 수 벡터 길이를 1 로. (길이 0 이면 아래쪽)
//==============================================================================
fn normalize_direction(direction: [f64; 3]) -> [f64; 3] {
	let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
	if length <= 0.0 {
		return [0.0, -1.0, 0.0];
	}
	[direction[0] / length, direction[1] / length, direction[2] / length]
}

//==============================================================================
/// 툰 빛 묶음. (머리말 참고)
///
/// # 예제
/// 재질마다 다른 그늘 색을 빛 묶음으로 주기
/// ```rust,no_run
/// use vanilla::{create_sphere_geometry, Matrix4, MeshBuilder, ToonDrawOptions, ToonPartOptions, ToonRenderer, ToonShade, Vector3};
///
/// fn draw_skin(gl: &glow::Context, renderer: &mut ToonRenderer) -> Option<()> {
/// 	let mut skin_light_set = renderer.create_light_set();
/// 	skin_light_set.set_shade(ToonShade { step: 0.55, feather: 0.04, color: [0.86, 0.6, 0.6], ..ToonShade::default() });
/// 	let mut builder = MeshBuilder::new();
/// 	builder.add_part(&create_sphere_geometry(0.5, 32, 20, std::f64::consts::PI), &Matrix4::create_identity(), "#f0d0c0", ToonPartOptions { has_outline: false, ..ToonPartOptions::default() });
/// 	let mesh = renderer.create_mesh(gl, &builder.read_arrays())?;
/// 	let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 0.0, 3.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// 	renderer.begin_frame_with_matrices(gl, &view_matrix, &Matrix4::create_perspective(0.8, 1.0, 0.1, 10.0));
/// 	renderer.draw_mesh(gl, &mesh, &Matrix4::create_identity(), ToonDrawOptions { light_set: Some(&skin_light_set), ..ToonDrawOptions::default() });
/// 	renderer.end_frame(gl);
/// 	Some(())
/// }
/// ```
//==============================================================================
#[derive(Debug)]
pub struct ToonLightSet {
	id: u32,
	version: u64,
	sun_direction: [f64; 3],
	sun_color: [f64; 3],
	sky_color: [f64; 3],
	ground_color: [f64; 3],
	fill_direction: [f64; 3],
	fill_color: [f64; 3],
	rim_light: ToonRimLight,
	shade_rim_light: ToonShadeRimLight,
	light_color_usage: ToonLightColorUsage,
	/// 빛마다 받은 몫을 1 로 막는지.
	is_light_intensity_limited: bool,
	specular: ToonSpecular,
	point_light_list: Vec<ToonPointLight>,
	point_light_limit: usize,
	shade_ramp: ToonShadeRamp,
	ramp_texture: Option<GlTexture>,
	light_model: ToonLightModel,
}

impl Default for ToonLightSet {
	//==============================================================================
	/// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonLightSet::new()
	}
}

//==============================================================================
/// 복제. (값은 같고 번호는 새로, 렌더러가 두 묶음을 같은 것으로 보지 않게)
//==============================================================================
impl Clone for ToonLightSet {
	//==============================================================================
	/// 복제. (값은 같고 번호는 새로)
	//==============================================================================
	fn clone(&self) -> Self {
		ToonLightSet {
			id: NEXT_LIGHT_SET_ID.fetch_add(1, Ordering::Relaxed),
			version: self.version,
			sun_direction: self.sun_direction,
			sun_color: self.sun_color,
			sky_color: self.sky_color,
			ground_color: self.ground_color,
			fill_direction: self.fill_direction,
			fill_color: self.fill_color,
			rim_light: self.rim_light,
			shade_rim_light: self.shade_rim_light,
			light_color_usage: self.light_color_usage,
			is_light_intensity_limited: self.is_light_intensity_limited,
			specular: self.specular,
			point_light_list: self.point_light_list.clone(),
			point_light_limit: self.point_light_limit,
			shade_ramp: self.shade_ramp.clone(),
			ramp_texture: self.ramp_texture,
			light_model: self.light_model,
		}
	}
}

impl ToonLightSet {
	//==============================================================================
	/// 생성. (기본은 반 램버트 0.5 에서 갈리는 두 단계 그늘, 그늘 색은 서늘한 보라)
	///
	/// # 돌려줌
	/// 새 빛 묶음. (번호는 묶음마다 다르다)
	//==============================================================================
	pub fn new() -> ToonLightSet {
		let mut light_set = ToonLightSet {
			id: NEXT_LIGHT_SET_ID.fetch_add(1, Ordering::Relaxed),
			version: 0,
			sun_direction: [-0.4, -1.0, -0.3],
			sun_color: [0.84, 0.8, 0.74],
			sky_color: [0.3, 0.33, 0.42],
			ground_color: [0.17, 0.16, 0.15],
			fill_direction: [0.0, -1.0, 0.0],
			fill_color: [0.0; 3],
			rim_light: ToonRimLight { color: [0.0; 3], power: 0.0, feather: 1.0, light_mask: 0.0 },
			shade_rim_light: ToonShadeRimLight { color: [0.0; 3], power: 0.0 },
			light_color_usage: ToonLightColorUsage::default(),
			is_light_intensity_limited: true,
			specular: ToonSpecular { strength: 0.0, exponent: 48.0, feather: 0.0, color: [1.0; 3] },
			point_light_list: Vec::new(),
			point_light_limit: TOON_MAXIMUM_POINT_LIGHTS,
			shade_ramp: ToonShadeRamp::default(),
			ramp_texture: None,
			light_model: ToonLightModel::default(),
		};
		light_set.set_shade(ToonShade::default());
		light_set
	}

	//==============================================================================
	/// 고친 판 올리기.
	//==============================================================================
	fn mark_changed(&mut self) {
		self.version += 1;
	}

	//==============================================================================
	/// 번호 반환. (묶음마다 다르다, 복제하면 새 번호)
	///
	/// # 돌려줌
	/// 묶음 번호.
	//==============================================================================
	pub fn get_id(&self) -> u32 {
		self.id
	}

	//==============================================================================
	/// 판 번호 반환. (고칠 때마다 오른다)
	///
	/// # 돌려줌
	/// 판 번호.
	//==============================================================================
	pub fn get_version(&self) -> u64 {
		self.version
	}

	//==============================================================================
	/// 해와 바탕빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형)
	///
	/// # 인자
	/// - `sun_direction`: 빛이 나아가는 쪽. [x, y, z]
	/// - `sun_color`: 해의 선형 색과 세기. [r, g, b]
	/// - `sky_color`: 위를 보는 면의 바탕빛. [r, g, b]
	/// - `ground_color`: 아래를 보는 면의 바탕빛. [r, g, b]
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f64; 3], sun_color: [f64; 3], sky_color: [f64; 3], ground_color: [f64; 3]) {
		self.sun_direction = sun_direction;
		self.sun_color = sun_color;
		self.sky_color = sky_color;
		self.ground_color = ground_color;
		self.mark_changed();
	}

	//==============================================================================
	/// 해가 비추는 방향 설정. (빛이 나아가는 쪽, 면에서 해 쪽 방향이면 부호를 뒤집어 준다)
	///
	/// # 인자
	/// - `sun_direction`: [x, y, z]
	//==============================================================================
	pub fn set_sun_direction(&mut self, sun_direction: [f64; 3]) {
		self.sun_direction = sun_direction;
		self.mark_changed();
	}

	//==============================================================================
	/// 해가 비추는 방향 반환. (ShadowMap::update_light_matrix 에 그대로 넘긴다)
	///
	/// # 돌려줌
	/// [x, y, z]
	//==============================================================================
	pub fn get_sun_direction(&self) -> [f64; 3] {
		self.sun_direction
	}

	//==============================================================================
	/// 해의 색 반환.
	///
	/// # 돌려줌
	/// 선형 [r, g, b].
	//==============================================================================
	pub fn get_sun_color(&self) -> [f64; 3] {
		self.sun_color
	}

	//==============================================================================
	/// 하늘색 반환.
	///
	/// # 돌려줌
	/// 선형 [r, g, b].
	//==============================================================================
	pub fn get_sky_color(&self) -> [f64; 3] {
		self.sky_color
	}

	//==============================================================================
	/// 땅색 반환.
	///
	/// # 돌려줌
	/// 선형 [r, g, b].
	//==============================================================================
	pub fn get_ground_color(&self) -> [f64; 3] {
		self.ground_color
	}

	//==============================================================================
	/// 채움 빛 설정. (빛이 나아가는 방향과 선형 색, 색이 0 이면 없음, 그림자를 받지 않음)
	///
	/// # 인자
	/// - `fill_direction`: 빛이 나아가는 쪽. [x, y, z] (길이 1 로 맞춘다, 길이 0 이면 아래쪽)
	/// - `fill_color`: 선형 [r, g, b].
	//==============================================================================
	pub fn set_fill_light(&mut self, fill_direction: [f64; 3], fill_color: [f64; 3]) {
		self.fill_direction = normalize_direction(fill_direction);
		self.fill_color = fill_color;
		self.mark_changed();
	}

	//==============================================================================
	/// 채움 빛 반환. ((방향, 색))
	///
	/// # 돌려줌
	/// (방향, 색). (방향은 길이 1)
	//==============================================================================
	pub fn get_fill_light(&self) -> ([f64; 3], [f64; 3]) {
		(self.fill_direction, self.fill_color)
	}

	//==============================================================================
	/// 림 빛 설정. (선형 색, 거듭제곱, 부드러움 0 단단한 테 ~ 1 부드러운 테, 해를 받는 쪽에만 둘 몫 0 ~ 1, 거듭제곱이 0 이하면 없음)
	///
	/// # 인자
	/// - `rim_color`: 더할 선형 색. [r, g, b]
	/// - `rim_power`: 거듭제곱. (클수록 테가 가늘다, 0 이면 없음)
	/// - `rim_feather`: 0 이면 단단한 테, 1 이면 부드러운 테. (원본 기본 1)
	/// - `rim_light_mask`: 1 이면 해를 받는 쪽에만 테를 둔다. (원본 기본 0)
	//==============================================================================
	pub fn set_rim_light(&mut self, rim_color: [f64; 3], rim_power: f64, rim_feather: f64, rim_light_mask: f64) {
		self.rim_light = ToonRimLight { color: rim_color, power: rim_power.max(0.0), feather: rim_feather.clamp(0.0, 1.0), light_mask: rim_light_mask.clamp(0.0, 1.0) };
		self.mark_changed();
	}

	//==============================================================================
	/// 림 빛 반환.
	///
	/// # 돌려줌
	/// 림 빛. (색, 거듭제곱, 부드러움, 해를 받는 쪽에만 둘 몫)
	//==============================================================================
	pub fn get_rim_light(&self) -> ToonRimLight {
		self.rim_light
	}

	//==============================================================================
	/// 그늘 쪽 림 빛 설정. (해와 등진 쪽의 테, 선형 색, 거듭제곱, 부드러움은 set_rim_light 의 것을 같이 쓴다, 거듭제곱이 0 이하면 없음)
	///
	/// # 인자
	/// - `shade_rim_color`: 더할 선형 색. [r, g, b]
	/// - `shade_rim_power`: 거듭제곱. (0 이면 없음)
	//==============================================================================
	pub fn set_shade_rim_light(&mut self, shade_rim_color: [f64; 3], shade_rim_power: f64) {
		self.shade_rim_light = ToonShadeRimLight { color: shade_rim_color, power: shade_rim_power.max(0.0) };
		self.mark_changed();
	}

	//==============================================================================
	/// 그늘 쪽 림 빛 반환.
	///
	/// # 돌려줌
	/// 그늘 쪽 림 빛. (색, 거듭제곱)
	//==============================================================================
	pub fn get_shade_rim_light(&self) -> ToonShadeRimLight {
		self.shade_rim_light
	}

	//==============================================================================
	/// 빛 색 쓰기 설정. (색마다 빛 색을 곱할지)
	///
	/// # 인자
	/// - `light_color_usage`: 색마다 빛 색을 곱할지의 참 거짓. (바탕, 그늘, 하이라이트, 림, 그늘 쪽 림, 매트캡, 천사 고리, 껍데기 바깥선)
	//==============================================================================
	pub fn set_light_color_usage(&mut self, light_color_usage: ToonLightColorUsage) {
		self.light_color_usage = light_color_usage;
		self.mark_changed();
	}

	//==============================================================================
	/// 빛 색 쓰기 반환.
	///
	/// # 돌려줌
	/// 색마다 빛 색을 곱할지의 참 거짓.
	//==============================================================================
	pub fn get_light_color_usage(&self) -> ToonLightColorUsage {
		self.light_color_usage
	}

	//==============================================================================
	/// 빛 세기 막기 설정. (true 면 빛마다 받은 몫을 1 로 막아 하얗게 날지 않게 한다, 기본 true, 유니티 툰 셰이더의 Limit Light Intensity)
	/// - 해와 채움 빛은 색의 채널마다 1 로 자른다. (주광의 saturate)
	/// - 점광과 스포트라이트는 감쇠까지 곱한 색의 세기(0.299 R + 0.587 G + 0.114 B)가 1 을 넘으면 세기로 나눈다. (추가 빛의 min(색, 색 / 세기))
	/// - 빛 꼴이 is_energy_normalized 면 π 로 나눈 뒤의 몫을 1 로 막는다.
	/// - 옛 게임 셰이더(빛 세기를 막지 않던 것)를 옮길 때나 세기 1 을 넘는 점광, HDR 블룸을 쓸 때는 set_light_intensity_limited(false).
	///
	/// # 인자
	/// - `is_limited`: true 면 막는다.
	//==============================================================================
	pub fn set_light_intensity_limited(&mut self, is_limited: bool) {
		self.is_light_intensity_limited = is_limited;
		self.mark_changed();
	}

	//==============================================================================
	/// 빛 세기를 막는지 반환.
	///
	/// # 돌려줌
	/// 막으면 true.
	//==============================================================================
	pub fn is_light_intensity_limited(&self) -> bool {
		self.is_light_intensity_limited
	}

	//==============================================================================
	/// 하이라이트 설정. (세기 0 이면 없음, 지수가 클수록 작은 점, 부드러움 0 단단한 툰 점 ~ 1 부드러운 블린 퐁, 선형 색)
	///
	/// # 인자
	/// - `specular_strength`: 세기. (0 이면 없음)
	/// - `specular_exponent`: 블린 퐁 지수. (원본 기본 48, 1 밑으로 내리지 않는다)
	/// - `specular_feather`: 0 ~ 1. (원본 기본 0, 단단한 점)
	/// - `specular_color`: 선형 [r, g, b]. (원본 기본 흰색)
	//==============================================================================
	pub fn set_specular(&mut self, specular_strength: f64, specular_exponent: f64, specular_feather: f64, specular_color: [f64; 3]) {
		self.specular = ToonSpecular { strength: specular_strength.max(0.0), exponent: specular_exponent.max(1.0), feather: specular_feather.clamp(0.0, 1.0), color: specular_color };
		self.mark_changed();
	}

	//==============================================================================
	/// 하이라이트 반환.
	///
	/// # 돌려줌
	/// 하이라이트. (세기, 지수, 부드러움, 색)
	//==============================================================================
	pub fn get_specular(&self) -> ToonSpecular {
		self.specular
	}

	//==============================================================================
	/// 점광과 스포트라이트 설정. (TOON_MAXIMUM_POINT_LIGHTS 를 넘는 것은 버린다, 범위와 지수와 각은 0 밑으로 내리지 않고 방향은 길이 1 로)
	///
	/// # 인자
	/// - `point_light_list`: 점광과 스포트라이트의 목록. (direction 이 Some 이면 스포트라이트)
	//==============================================================================
	pub fn set_point_lights(&mut self, point_light_list: &[ToonPointLight]) {
		self.point_light_list = point_light_list
			.iter()
			.take(TOON_MAXIMUM_POINT_LIGHTS)
			.map(|point_light| ToonPointLight {
				range: point_light.range.max(0.0),
				decay: point_light.decay.max(0.0),
				range_exponent: point_light.range_exponent.max(0.0001),
				wrap: point_light.wrap.max(0.0),
				direction: point_light.direction.map(normalize_direction),
				inner_cone_angle: point_light.inner_cone_angle.max(0.0),
				outer_cone_angle: point_light.outer_cone_angle.max(0.0001),
				shadow_near: point_light.shadow_near.max(0.001),
				..*point_light
			})
			.collect();
		self.mark_changed();
	}

	//==============================================================================
	/// 점광 반환. (설정한 것 그대로, 한도로 자르기 전)
	///
	/// # 돌려줌
	/// set_point_lights 가 채운 꼴의 목록.
	//==============================================================================
	pub fn get_point_lights(&self) -> &[ToonPointLight] {
		&self.point_light_list
	}

	//==============================================================================
	/// 그림자 아틀라스에 줄 빛 꼴 읽기. (점광 목록 차례 그대로, ShadowAtlas::update_lights 에 넘긴다)
	///
	/// # 돌려줌
	/// 점광마다 하나인 그림자 아틀라스 빛 꼴. (자리, 방향, 바깥 원뿔 반각, 범위, 그림자의 가까운 거리, 그림자를 드리울지)
	//==============================================================================
	pub fn read_shadow_light_sources(&self) -> Vec<ShadowAtlasLightSource> {
		self.point_light_list
			.iter()
			.map(|point_light| ShadowAtlasLightSource { position: point_light.position, direction: point_light.direction, outer_cone_angle: point_light.outer_cone_angle, range: point_light.range, shadow_near: point_light.shadow_near, casts_shadow: point_light.casts_shadow })
			.collect()
	}

	//==============================================================================
	/// 셰이더에 넣을 점광 수의 한도 설정. (0 ~ TOON_MAXIMUM_POINT_LIGHTS, 가벼운 길에서 줄인다)
	///
	/// # 인자
	/// - `point_light_limit`: 한도.
	//==============================================================================
	pub fn set_point_light_limit(&mut self, point_light_limit: usize) {
		self.point_light_limit = point_light_limit.min(TOON_MAXIMUM_POINT_LIGHTS);
		self.mark_changed();
	}

	//==============================================================================
	/// 셰이더에 넣을 점광 수. (설정한 수와 한도 가운데 작은 것)
	///
	/// # 돌려줌
	/// 셰이더에 넣을 점광 수.
	//==============================================================================
	pub fn get_point_light_count(&self) -> usize {
		self.point_light_list.len().min(self.point_light_limit)
	}

	//==============================================================================
	/// 그늘 셋 설정. (ToonShade, 바탕은 흰색 [1, 1, 1])
	///
	/// # 인자
	/// - `shade`: 그늘 셋. (경계와 부드러움과 색, 2 차 그늘, 반 램버트)
	//==============================================================================
	pub fn set_shade(&mut self, shade: ToonShade) {
		if shade.second_step > 0.0 {
			self.set_shade_ramp(&[0.0, shade.second_step, shade.step], &[shade.second_color, shade.color, [1.0; 3]], &[0.0, shade.second_feather, shade.feather], shade.is_half_lambert);
			return;
		}
		self.set_shade_ramp(&[0.0, shade.step], &[shade.color, [1.0; 3]], &[0.0, shade.feather], shade.is_half_lambert);
	}

	//==============================================================================
	/// 그늘 램프 설정. (칸마다 자리, 선형 색, 부드러움, 칸은 2 ~ TOON_MAXIMUM_BAND_STEPS, 둘보다 적으면 부드러운 램버트, 자리는 오름차순)
	///
	/// # 인자
	/// - `positions`: 칸마다의 자리. (빛을 받는 몫 x)
	/// - `colors`: 칸마다의 선형 [r, g, b] 배.
	/// - `feathers`: 칸마다의 부드러움. (그 칸으로 넘어가는 너비)
	/// - `is_half_lambert`: x 를 dot x 0.5 + 0.5 로 셀지. (아니면 max(dot, 0))
	//==============================================================================
	pub fn set_shade_ramp(&mut self, positions: &[f64], colors: &[[f64; 3]], feathers: &[f64], is_half_lambert: bool) {
		let stop_count = positions.len().min(colors.len()).min(TOON_MAXIMUM_BAND_STEPS);
		self.shade_ramp = if stop_count < 2 {
			ToonShadeRamp { positions: Vec::new(), colors: Vec::new(), feathers: Vec::new(), is_half_lambert }
		}
		else {
			ToonShadeRamp {
				positions: positions[..stop_count].to_vec(),
				colors: colors[..stop_count].to_vec(),
				feathers: (0..stop_count).map(|stop_index| feathers.get(stop_index).copied().unwrap_or(0.0).max(0.0)).collect(),
				is_half_lambert,
			}
		};
		self.mark_changed();
	}

	//==============================================================================
	/// 그늘 램프 끄기. (부드러운 램버트)
	//==============================================================================
	pub fn clear_shade_ramp(&mut self) {
		self.set_shade_ramp(&[], &[], &[], false);
	}

	//==============================================================================
	/// 그늘 램프 반환. (칸이 비면 부드러운 램버트)
	///
	/// # 돌려줌
	/// 그늘 램프. (칸마다의 자리, 색, 부드러움과 반 램버트인지)
	//==============================================================================
	pub fn get_shade_ramp(&self) -> &ToonShadeRamp {
		&self.shade_ramp
	}

	//==============================================================================
	/// 고른 칸 설정. (max(dot(N, L), 0) 을 칸 수로 반올림한 회색 칸, 1 ~ TOON_MAXIMUM_BAND_STEPS - 1, 0 이면 부드러운 램버트)
	///
	/// # 인자
	/// - `band_count`: 칸 수. (0 이면 부드럽게)
	//==============================================================================
	pub fn set_band_count(&mut self, band_count: u32) {
		let count = (band_count as usize).min(TOON_MAXIMUM_BAND_STEPS - 1);
		if count == 0 {
			self.clear_shade_ramp();
			return;
		}
		let mut positions = vec![0.0];
		let mut colors = vec![[0.0; 3]];
		let mut feathers = vec![0.0];
		for band_index in 1..=count {
			let value = band_index as f64 / count as f64;
			positions.push((band_index as f64 - 0.5) / count as f64);
			colors.push([value; 3]);
			feathers.push(0.0);
		}
		self.set_shade_ramp(&positions, &colors, &feathers, false);
	}

	//==============================================================================
	/// 회색 칸 램프 설정. (문턱은 값보다 하나 적다, 값은 TOON_MAXIMUM_BAND_STEPS 까지, 첫 문턱 밑이 첫 값, 마지막 문턱 위가 마지막 값)
	///
	/// # 인자
	/// - `thresholds`: 오름차순 문턱.
	/// - `values`: 칸마다의 받는 몫.
	/// - `is_half_lambert`: dot x 0.5 + 0.5 를 문턱에 댈지.
	//==============================================================================
	pub fn set_band_ramp(&mut self, thresholds: &[f64], values: &[f64], is_half_lambert: bool) {
		let value_count = values.len().min(TOON_MAXIMUM_BAND_STEPS);
		let mut positions = vec![0.0];
		let mut colors = Vec::new();
		let mut feathers = vec![0.0];
		for (value_index, value) in values.iter().take(value_count).enumerate() {
			colors.push([*value; 3]);
			if value_index > 0 {
				positions.push(thresholds.get(value_index - 1).copied().unwrap_or(1e9));
				feathers.push(0.0);
			}
		}
		self.set_shade_ramp(&positions, &colors, &feathers, is_half_lambert);
	}

	//==============================================================================
	/// 램프 텍스처 설정. (칸 대신 텍스처의 가로를 읽는다, None 이면 칸으로, ToonRenderer::create_texture 로 지은 것, 왼쪽이 그늘 오른쪽이 빛)
	///
	/// # 인자
	/// - `ramp_texture`: 램프 텍스처나 None.
	/// - `is_half_lambert`: x 를 반 램버트로 셀지. (원본 기본 true)
	//==============================================================================
	pub fn set_ramp_texture(&mut self, ramp_texture: Option<GlTexture>, is_half_lambert: bool) {
		self.ramp_texture = ramp_texture;
		self.shade_ramp.is_half_lambert = is_half_lambert;
		self.mark_changed();
	}

	//==============================================================================
	/// 램프 텍스처 반환. (없으면 None)
	///
	/// # 돌려줌
	/// 램프 텍스처. (없으면 None)
	//==============================================================================
	pub fn get_ramp_texture(&self) -> Option<GlTexture> {
		self.ramp_texture
	}

	//==============================================================================
	/// 빛 꼴 설정.
	///
	/// # 인자
	/// - `light_model`: 빛 꼴. (is_energy_normalized)
	//==============================================================================
	pub fn set_light_model(&mut self, light_model: ToonLightModel) {
		self.light_model = light_model;
		self.mark_changed();
	}

	//==============================================================================
	/// 빛 꼴 반환.
	///
	/// # 돌려줌
	/// 빛 꼴. (is_energy_normalized)
	//==============================================================================
	pub fn get_light_model(&self) -> ToonLightModel {
		self.light_model
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 그림자를 드리우는 점광만 아틀라스 빛으로 넘기고, 빛 색 쓰기는 기본이 모두 켬인지.
	//==============================================================================
	#[test]
	fn shadow_light_sources_and_color_usage() {
		let mut light_set = ToonLightSet::new();
		light_set.set_point_lights(&[
			ToonPointLight { position: [1.0, 2.0, 3.0], range: 4.0, ..ToonPointLight::default() },
			ToonPointLight { position: [0.0, 3.0, 0.0], range: 6.0, direction: Some([0.0, -1.0, 0.0]), outer_cone_angle: 0.5, casts_shadow: true, shadow_near: 0.0, ..ToonPointLight::default() },
		]);
		let light_source_list = light_set.read_shadow_light_sources();
		assert_eq!(light_source_list.len(), 2);
		assert!(!light_source_list[0].casts_shadow);
		assert!(light_source_list[1].casts_shadow);
		assert!(light_source_list[1].shadow_near > 0.0);
		let usage = light_set.get_light_color_usage();
		assert!(usage.base && usage.shade && usage.highlight && usage.rim && usage.shade_rim && usage.matcap && usage.angel_ring && usage.outline);
		light_set.set_light_color_usage(ToonLightColorUsage { outline: false, ..ToonLightColorUsage::default() });
		assert!(!light_set.get_light_color_usage().outline);
		light_set.set_shade_rim_light([0.2, 0.3, 0.6], 2.5);
		assert_eq!(light_set.get_shade_rim_light().power, 2.5);
		assert!(light_set.is_light_intensity_limited());
		light_set.set_light_intensity_limited(false);
		assert!(!light_set.is_light_intensity_limited());
		assert!(!light_set.clone().is_light_intensity_limited());
	}

	//==============================================================================
	/// 그늘 셋, 회색 칸, 고른 칸이 램프 칸을 바르게 짓고 판 번호가 오르는지.
	//==============================================================================
	#[test]
	fn shade_ramp_presets_follow_limits() {
		let mut light_set = ToonLightSet::new();
		assert_eq!(light_set.get_shade_ramp().positions, vec![0.0, 0.5]);
		assert!(light_set.get_shade_ramp().is_half_lambert);
		let first_version = light_set.get_version();
		light_set.set_shade(ToonShade { second_step: 0.2, ..ToonShade::default() });
		assert_eq!(light_set.get_shade_ramp().positions, vec![0.0, 0.2, 0.5]);
		assert_eq!(light_set.get_shade_ramp().colors[2], [1.0; 3]);
		assert!(light_set.get_version() > first_version);
		light_set.set_band_ramp(&[0.25, 0.5, 0.75], &[0.3725, 0.6471, 0.8627, 1.0], true);
		assert_eq!(light_set.get_shade_ramp().positions, vec![0.0, 0.25, 0.5, 0.75]);
		assert_eq!(light_set.get_shade_ramp().colors[1], [0.6471; 3]);
		light_set.set_band_ramp(&[0.5], &[0.2, 0.6, 1.0], false);
		assert_eq!(light_set.get_shade_ramp().positions, vec![0.0, 0.5, 1e9]);
		light_set.set_band_count(3);
		assert_eq!(light_set.get_shade_ramp().positions.len(), 4);
		assert!((light_set.get_shade_ramp().positions[1] - 0.5 / 3.0).abs() < 0.0001);
		light_set.set_band_count(0);
		assert!(light_set.get_shade_ramp().positions.is_empty());
	}

	//==============================================================================
	/// 점광이 상한과 한도를 따르고 스포트라이트 방향을 길이 1 로 맞추는지.
	//==============================================================================
	#[test]
	fn point_lights_follow_limits_and_spots() {
		let mut light_set = ToonLightSet::new();
		let point_light_list = vec![ToonPointLight { range: -1.0, direction: Some([0.0, -2.0, 0.0]), ..ToonPointLight::default() }; 10];
		light_set.set_point_lights(&point_light_list);
		assert_eq!(light_set.get_point_lights().len(), TOON_MAXIMUM_POINT_LIGHTS);
		assert_eq!(light_set.get_point_lights()[0].range, 0.0);
		assert_eq!(light_set.get_point_lights()[0].direction, Some([0.0, -1.0, 0.0]));
		light_set.set_point_light_limit(4);
		assert_eq!(light_set.get_point_light_count(), 4);
		assert_ne!(ToonLightSet::new().get_id(), light_set.get_id());
	}
}
