//==============================================================================
// 툰 빛 묶음. (vanilla.js experimental/graphics/toon/toonlightset.js 의 ToonLightSet)
// - ToonRenderer 가 몸을 칠할 빛과 툰 빛 반응을 한 덩어리로 든다. 렌더러는 기본 묶음 하나를 들고, set_light 같은 렌더러의 빛 설정은 그 묶음을 고친다.
//   그리기마다 ToonDrawOptions 의 light_set 으로 다른 묶음을 줄 수 있다. (재질마다 그늘 색이 다를 때)
// - 해(주광): 방향(sun_direction)은 빛이 나아가는 쪽이다. 면에서 해 쪽을 가리키는 방향의 반대다. (위에서 내리쬐면 [0, -1, 0]) 그림자를 받는다.
// - 바탕빛: 하늘색과 땅색을 노멀의 위아래(N.y x 0.5 + 0.5)로 섞는다. (반구 빛)
// - 그늘 램프(셀 셰이딩): 빛을 받는 몫 x 를 램프에 대어 색을 고른다. x 는 반 램버트면 dot(N, L) x 0.5 + 0.5, 아니면 max(dot(N, L), 0) 이고,
//   그리기의 shade_offset 을 더한 뒤 해는 그림자(받는 몫 0 ~ 1)를 곱한다. (그림자가 검정이 아니라 그늘 색으로 들어간다, 유니티 툰 셰이더의 System Shadows)
//   램프는 칸(stop)마다 자리, 색(선형 배), 부드러움이다. 칸 i 의 색은 x 가 (자리 - 부드러움) 에서 자리까지 오를 때 앞 칸 색에서 넘어간다. (유니티 툰 셰이더의 Step 과 Feather)
//   부드러움이 화소 하나 너비(fwidth)보다 작으면 그 너비로 막아 경계가 계단지지 않는다.
//   set_shade 는 바탕, 1 차 그늘, 2 차 그늘 셋으로, set_shade_ramp 는 칸을 바로, set_band_count 는 고른 칸으로, set_band_ramp 는 회색 칸으로 램프를 짓는다.
//   set_ramp_texture 를 주면 칸 대신 그 텍스처의 가로(x 를 u 로, v 는 0.5)를 읽는다. 칸이 없으면(clear_shade_ramp, set_band_count(0)) 부드러운 램버트다.
// - 빛마다: 해는 램프 색을 통째로 받아 그늘도 해의 색으로 물든다. 채움 빛, 점광, 스포트라이트는 램프의 밝기 단계(바닥 칸이 0, 맨 위 칸이 1)를 받아 등진 면을 밝히지 않는다.
// - 채움 빛(fill): 그림자를 받지 않는 둘째 방향광. 방향의 뜻은 해와 같다. 기본 색 0 이면 없다.
// - 하이라이트(정반사): 블린 퐁 pow(max(dot(N, H), 0), exponent) 를 0.5 에서 가른 단단한 점(부드러움 0)에서 그대로의 부드러운 빛(부드러움 1)까지. 색 x 세기 x 빛 색 x 빛을 받는 단계.
//   해와 점광, 스포트라이트 모두에 선다. 기본 세기 0 이면 없다.
// - 림 빛(rim): pow(1 - max(dot(N, V), 0), power) 를 0.5 에서 가른 단단한 테(부드러움 0)에서 부드러운 테(1)까지. light_mask 가 1 이면 해를 받는 쪽에만. 바탕색과 상관없이 더한다.
// - 점광과 스포트라이트: TOON_MAXIMUM_POINT_LIGHTS 까지. (ToonPointLight)
//   감쇠는 창 x 거리 감쇠다. 창은 range 가 0 보다 크면 (1 - (거리 / range)^range_exponent) 를 0 ~ 1 로 막아 제곱, 0 이면 1. 거리 감쇠는 decay 가 0 보다 크면 1 / max(거리^decay, 0.01), 0 이면 1.
//   direction(빛이 나아가는 쪽)이 있으면 스포트라이트다. 각 감쇠는 glTF KHR_lights_punctual 꼴로 clamp(cos 각 x scale + offset, 0, 1) 의 제곱이다.
//   받는 몫은 감싸기(wrap) (dot(N, L) + wrap) / (1 + wrap) 이다. (0 이면 램버트, GPU Gems 1 의 wrap lighting)
// - 빛 꼴: set_light_model(ToonLightModel { is_energy_normalized }) 가 true 면 바탕색 x 빛을 π 로 나누고 하이라이트에 (exponent + 8) / 8π 를 곱한다.
// - 색은 모두 선형 색이다. (HDR 이면 1 을 넘어도 된다) 램프 텍스처는 sRGB 로 보고 선형으로 바꿔 읽는다.
// - 고치면 판 번호(get_version)가 오른다. 렌더러는 묶음 번호(get_id)와 판 번호가 같으면 다시 올리지 않는다.
//==============================================================================
use std::sync::atomic::{AtomicU32, Ordering};

use crate::core::graphic::GlTexture;

// 한 그리기에 받는 점광과 스포트라이트 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_POINT_LIGHTS: usize = 8;

// 그늘 램프 칸 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_BAND_STEPS: usize = 8;

// 빛 묶음마다 다른 번호를 주는 셈. (0 은 쓰지 않는다)
static NEXT_LIGHT_SET_ID: AtomicU32 = AtomicU32::new(1);

//==============================================================================
// 점광이나 스포트라이트 하나. (원본 setPointLights 의 객체 하나)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonPointLight {
	// 월드 자리.
	pub position: [f32; 3],
	// 선형 색.
	pub color: [f32; 3],
	// 색에 곱할 세기.
	pub intensity: f32,
	// 창의 범위. (0 이면 끝없음)
	pub range: f32,
	// 거리 감쇠 지수. (0 이면 거리 감쇠 없음)
	pub decay: f32,
	// 창 꼴 지수.
	pub range_exponent: f32,
	// 감싸기 몫. (0 이면 램버트)
	pub wrap: f32,
	// 빛이 나아가는 쪽. (Some 이면 스포트라이트)
	pub direction: Option<[f32; 3]>,
	// 스포트라이트 안쪽 원뿔 반각. (라디안, 여기까지 다 밝다)
	pub inner_cone_angle: f32,
	// 스포트라이트 바깥 원뿔 반각. (라디안, 여기서 0 이 된다)
	pub outer_cone_angle: f32,
}

impl Default for ToonPointLight {
	//==============================================================================
	// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonPointLight { position: [0.0; 3], color: [1.0; 3], intensity: 1.0, range: 0.0, decay: 2.0, range_exponent: 4.0, wrap: 0.0, direction: None, inner_cone_angle: 0.0, outer_cone_angle: std::f32::consts::FRAC_PI_4 }
	}
}

//==============================================================================
// 빛 꼴. (is_energy_normalized 바탕색 x 빛을 π 로 나눌지)
//==============================================================================
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ToonLightModel {
	pub is_energy_normalized: bool,
}

//==============================================================================
// 그늘 셋. (유니티 툰 셰이더의 바탕, 1 차 그늘, 2 차 그늘과 그 경계의 Step, Feather, second_step 이 0 이하면 2 차 그늘 없음)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonShade {
	// 바탕과 1 차 그늘의 경계. (빛을 받는 몫 x 의 자리, 반 램버트면 0.5 가 해와 직각인 면)
	pub step: f32,
	// 바탕과 1 차 그늘 경계의 부드러움.
	pub feather: f32,
	// 1 차 그늘 색. (바탕색과 빛에 곱하는 선형 배)
	pub color: [f32; 3],
	// 1 차와 2 차 그늘의 경계. (0 이하면 2 차 그늘 없음)
	pub second_step: f32,
	// 1 차와 2 차 그늘 경계의 부드러움.
	pub second_feather: f32,
	// 2 차 그늘 색.
	pub second_color: [f32; 3],
	// x 를 반 램버트로 셀지.
	pub is_half_lambert: bool,
}

impl Default for ToonShade {
	//==============================================================================
	// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonShade { step: 0.5, feather: 0.02, color: [0.5, 0.48, 0.66], second_step: 0.0, second_feather: 0.02, second_color: [0.32, 0.3, 0.48], is_half_lambert: true }
	}
}

//==============================================================================
// 그늘 램프. (칸마다 자리, 선형 색, 부드러움, 칸이 비면 부드러운 램버트, 첫 칸의 자리와 부드러움은 쓰지 않는다)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToonShadeRamp {
	pub positions: Vec<f32>,
	pub colors: Vec<[f32; 3]>,
	pub feathers: Vec<f32>,
	pub is_half_lambert: bool,
}

//==============================================================================
// 림 빛. (선형 색, 거듭제곱, 부드러움 0 ~ 1, 해를 받는 쪽에만 둘 몫 0 ~ 1)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonRimLight {
	pub color: [f32; 3],
	pub power: f32,
	pub feather: f32,
	pub light_mask: f32,
}

//==============================================================================
// 하이라이트. (세기, 블린 퐁 지수, 부드러움 0 ~ 1, 선형 색)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonSpecular {
	pub strength: f32,
	pub exponent: f32,
	pub feather: f32,
	pub color: [f32; 3],
}

//==============================================================================
// 3 수 벡터 길이를 1 로. (길이 0 이면 아래쪽)
//==============================================================================
fn normalize_direction(direction: [f32; 3]) -> [f32; 3] {
	let length = (direction[0] * direction[0] + direction[1] * direction[1] + direction[2] * direction[2]).sqrt();
	if length <= 0.0 {
		return [0.0, -1.0, 0.0];
	}
	[direction[0] / length, direction[1] / length, direction[2] / length]
}

//==============================================================================
// 툰 빛 묶음. (머리말 참고)
//==============================================================================
#[derive(Debug)]
pub struct ToonLightSet {
	id: u32,
	version: u64,
	sun_direction: [f32; 3],
	sun_color: [f32; 3],
	sky_color: [f32; 3],
	ground_color: [f32; 3],
	fill_direction: [f32; 3],
	fill_color: [f32; 3],
	rim_light: ToonRimLight,
	specular: ToonSpecular,
	point_light_list: Vec<ToonPointLight>,
	point_light_limit: usize,
	shade_ramp: ToonShadeRamp,
	ramp_texture: Option<GlTexture>,
	light_model: ToonLightModel,
}

impl Default for ToonLightSet {
	//==============================================================================
	// 기본값. (원본의 기본 꼴과 같다)
	//==============================================================================
	fn default() -> Self {
		ToonLightSet::new()
	}
}

//==============================================================================
// 복제. (값은 같고 번호는 새로, 렌더러가 두 묶음을 같은 것으로 보지 않게)
//==============================================================================
impl Clone for ToonLightSet {
	//==============================================================================
	// 복제. (값은 같고 번호는 새로)
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
	// 생성. (기본은 반 램버트 0.5 에서 갈리는 두 단계 그늘, 그늘 색은 서늘한 보라)
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
	// 고친 판 올리기.
	//==============================================================================
	fn mark_changed(&mut self) {
		self.version += 1;
	}

	//==============================================================================
	// 번호 반환. (묶음마다 다르다, 복제하면 새 번호)
	//==============================================================================
	pub fn get_id(&self) -> u32 {
		self.id
	}

	//==============================================================================
	// 판 번호 반환. (고칠 때마다 오른다)
	//==============================================================================
	pub fn get_version(&self) -> u64 {
		self.version
	}

	//==============================================================================
	// 해와 바탕빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색, 모두 선형)
	//==============================================================================
	pub fn set_light(&mut self, sun_direction: [f32; 3], sun_color: [f32; 3], sky_color: [f32; 3], ground_color: [f32; 3]) {
		self.sun_direction = sun_direction;
		self.sun_color = sun_color;
		self.sky_color = sky_color;
		self.ground_color = ground_color;
		self.mark_changed();
	}

	//==============================================================================
	// 해가 비추는 방향 설정. (빛이 나아가는 쪽, 면에서 해 쪽 방향이면 부호를 뒤집어 준다)
	//==============================================================================
	pub fn set_sun_direction(&mut self, sun_direction: [f32; 3]) {
		self.sun_direction = sun_direction;
		self.mark_changed();
	}

	//==============================================================================
	// 해가 비추는 방향 반환. (ShadowMap::update_light_matrix 에 그대로 넘긴다)
	//==============================================================================
	pub fn get_sun_direction(&self) -> [f32; 3] {
		self.sun_direction
	}

	//==============================================================================
	// 해의 색 반환.
	//==============================================================================
	pub fn get_sun_color(&self) -> [f32; 3] {
		self.sun_color
	}

	//==============================================================================
	// 하늘색 반환.
	//==============================================================================
	pub fn get_sky_color(&self) -> [f32; 3] {
		self.sky_color
	}

	//==============================================================================
	// 땅색 반환.
	//==============================================================================
	pub fn get_ground_color(&self) -> [f32; 3] {
		self.ground_color
	}

	//==============================================================================
	// 채움 빛 설정. (빛이 나아가는 방향과 선형 색, 색이 0 이면 없음, 그림자를 받지 않음)
	//==============================================================================
	pub fn set_fill_light(&mut self, fill_direction: [f32; 3], fill_color: [f32; 3]) {
		self.fill_direction = normalize_direction(fill_direction);
		self.fill_color = fill_color;
		self.mark_changed();
	}

	//==============================================================================
	// 채움 빛 반환. ((방향, 색))
	//==============================================================================
	pub fn get_fill_light(&self) -> ([f32; 3], [f32; 3]) {
		(self.fill_direction, self.fill_color)
	}

	//==============================================================================
	// 림 빛 설정. (선형 색, 거듭제곱, 부드러움 0 단단한 테 ~ 1 부드러운 테, 해를 받는 쪽에만 둘 몫 0 ~ 1, 거듭제곱이 0 이하면 없음)
	//==============================================================================
	pub fn set_rim_light(&mut self, rim_color: [f32; 3], rim_power: f32, rim_feather: f32, rim_light_mask: f32) {
		self.rim_light = ToonRimLight { color: rim_color, power: rim_power.max(0.0), feather: rim_feather.clamp(0.0, 1.0), light_mask: rim_light_mask.clamp(0.0, 1.0) };
		self.mark_changed();
	}

	//==============================================================================
	// 림 빛 반환.
	//==============================================================================
	pub fn get_rim_light(&self) -> ToonRimLight {
		self.rim_light
	}

	//==============================================================================
	// 하이라이트 설정. (세기 0 이면 없음, 지수가 클수록 작은 점, 부드러움 0 단단한 툰 점 ~ 1 부드러운 블린 퐁, 선형 색)
	//==============================================================================
	pub fn set_specular(&mut self, specular_strength: f32, specular_exponent: f32, specular_feather: f32, specular_color: [f32; 3]) {
		self.specular = ToonSpecular { strength: specular_strength.max(0.0), exponent: specular_exponent.max(1.0), feather: specular_feather.clamp(0.0, 1.0), color: specular_color };
		self.mark_changed();
	}

	//==============================================================================
	// 하이라이트 반환.
	//==============================================================================
	pub fn get_specular(&self) -> ToonSpecular {
		self.specular
	}

	//==============================================================================
	// 점광과 스포트라이트 설정. (TOON_MAXIMUM_POINT_LIGHTS 를 넘는 것은 버린다, 범위와 지수와 각은 0 밑으로 내리지 않고 방향은 길이 1 로)
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
				..*point_light
			})
			.collect();
		self.mark_changed();
	}

	//==============================================================================
	// 점광 반환. (설정한 것 그대로, 한도로 자르기 전)
	//==============================================================================
	pub fn get_point_lights(&self) -> &[ToonPointLight] {
		&self.point_light_list
	}

	//==============================================================================
	// 셰이더에 넣을 점광 수의 한도 설정. (0 ~ TOON_MAXIMUM_POINT_LIGHTS, 가벼운 길에서 줄인다)
	//==============================================================================
	pub fn set_point_light_limit(&mut self, point_light_limit: usize) {
		self.point_light_limit = point_light_limit.min(TOON_MAXIMUM_POINT_LIGHTS);
		self.mark_changed();
	}

	//==============================================================================
	// 셰이더에 넣을 점광 수. (설정한 수와 한도 가운데 작은 것)
	//==============================================================================
	pub fn get_point_light_count(&self) -> usize {
		self.point_light_list.len().min(self.point_light_limit)
	}

	//==============================================================================
	// 그늘 셋 설정. (ToonShade, 바탕은 흰색 [1, 1, 1])
	//==============================================================================
	pub fn set_shade(&mut self, shade: ToonShade) {
		if shade.second_step > 0.0 {
			self.set_shade_ramp(&[0.0, shade.second_step, shade.step], &[shade.second_color, shade.color, [1.0; 3]], &[0.0, shade.second_feather, shade.feather], shade.is_half_lambert);
			return;
		}
		self.set_shade_ramp(&[0.0, shade.step], &[shade.color, [1.0; 3]], &[0.0, shade.feather], shade.is_half_lambert);
	}

	//==============================================================================
	// 그늘 램프 설정. (칸마다 자리, 선형 색, 부드러움, 칸은 2 ~ TOON_MAXIMUM_BAND_STEPS, 둘보다 적으면 부드러운 램버트, 자리는 오름차순)
	//==============================================================================
	pub fn set_shade_ramp(&mut self, positions: &[f32], colors: &[[f32; 3]], feathers: &[f32], is_half_lambert: bool) {
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
	// 그늘 램프 끄기. (부드러운 램버트)
	//==============================================================================
	pub fn clear_shade_ramp(&mut self) {
		self.set_shade_ramp(&[], &[], &[], false);
	}

	//==============================================================================
	// 그늘 램프 반환. (칸이 비면 부드러운 램버트)
	//==============================================================================
	pub fn get_shade_ramp(&self) -> &ToonShadeRamp {
		&self.shade_ramp
	}

	//==============================================================================
	// 고른 칸 설정. (max(dot(N, L), 0) 을 칸 수로 반올림한 회색 칸, 1 ~ TOON_MAXIMUM_BAND_STEPS - 1, 0 이면 부드러운 램버트)
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
			let value = band_index as f32 / count as f32;
			positions.push((band_index as f32 - 0.5) / count as f32);
			colors.push([value; 3]);
			feathers.push(0.0);
		}
		self.set_shade_ramp(&positions, &colors, &feathers, false);
	}

	//==============================================================================
	// 회색 칸 램프 설정. (문턱은 값보다 하나 적다, 값은 TOON_MAXIMUM_BAND_STEPS 까지, 첫 문턱 밑이 첫 값, 마지막 문턱 위가 마지막 값)
	//==============================================================================
	pub fn set_band_ramp(&mut self, thresholds: &[f32], values: &[f32], is_half_lambert: bool) {
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
	// 램프 텍스처 설정. (칸 대신 텍스처의 가로를 읽는다, None 이면 칸으로, ToonRenderer::create_texture 로 지은 것, 왼쪽이 그늘 오른쪽이 빛)
	//==============================================================================
	pub fn set_ramp_texture(&mut self, ramp_texture: Option<GlTexture>, is_half_lambert: bool) {
		self.ramp_texture = ramp_texture;
		self.shade_ramp.is_half_lambert = is_half_lambert;
		self.mark_changed();
	}

	//==============================================================================
	// 램프 텍스처 반환. (없으면 None)
	//==============================================================================
	pub fn get_ramp_texture(&self) -> Option<GlTexture> {
		self.ramp_texture
	}

	//==============================================================================
	// 빛 꼴 설정.
	//==============================================================================
	pub fn set_light_model(&mut self, light_model: ToonLightModel) {
		self.light_model = light_model;
		self.mark_changed();
	}

	//==============================================================================
	// 빛 꼴 반환.
	//==============================================================================
	pub fn get_light_model(&self) -> ToonLightModel {
		self.light_model
	}
}

#[cfg(test)]
mod tests {
	use super::*;

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
