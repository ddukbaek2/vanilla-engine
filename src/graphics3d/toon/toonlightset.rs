//==============================================================================
// 툰 빛 묶음. (vanilla.js experimental/graphics/toon/toonlightset.js 의 ToonLightSet)
// - ToonRenderer 가 몸을 칠할 빛 값을 한 덩어리로 든다. 렌더러는 기본 묶음 하나를 들고, set_light 같은 렌더러의 빛 설정은 그 묶음을 고친다.
//   그리기마다 ToonDrawOptions 의 light_set 으로 다른 묶음을 줄 수 있다. (판과 모형의 빛이 다를 때)
// - 해: 방향(sun_direction)은 빛이 나아가는 쪽이다. 해에서 나와 면으로 가는 방향이고, 면에서 해 쪽을 가리키는 방향의 반대다. (three.js DirectionalLight 의 position 에서 target 으로 가는 쪽)
//   면이 받는 몫은 max(dot(N, -sun_direction), 0) 이다. 위에서 내리쬐면 [0, -1, 0] 이다.
// - 바탕빛: 하늘색과 땅색을 노멀의 위아래(N.y x 0.5 + 0.5)로 섞는다. (반구 빛)
// - 채움 빛(fill): 그림자를 받지 않는 둘째 방향광. 방향의 뜻은 해와 같다. 기본 색 0 이면 없다.
// - 림 빛(rim): 보는 쪽과 노멀이 비껴갈수록 세지는 테 빛 pow(1 - max(dot(N, V), 0), power). 빛처럼 바탕색에 곱한다. 기본 세기 0 이면 없다.
// - 정반사: 해의 블린-퐁 pow(max(dot(N, H), 0), exponent) x strength. 그림자를 받는다. 기본 0 이면 없다.
// - 점광: TOON_MAXIMUM_POINT_LIGHTS 까지. (ToonPointLight)
//   감쇠는 창 x 거리 감쇠다. 창은 range 가 0 보다 크면 (1 - (거리 / range)^range_exponent) 를 0 ~ 1 로 막아 제곱, 0 이면 1. 거리 감쇠는 decay 가 0 보다 크면 1 / max(거리^decay, 0.01), 0 이면 1.
//   기본(decay 2, range_exponent 4)은 three.js 와 언리얼, 고도 4 의 점광 꼴이다. decay 0, range_exponent 1 이면 (1 - 거리 / range)^2 꼴(고도 3 의 감쇠 1)이 된다.
//   받는 몫은 감싸기(wrap) 빛 (dot(N, L) + wrap) / (1 + wrap) 이다. (0 이면 램버트, GPU Gems 1 의 wrap lighting)
// - 띠: set_band_count(n) 은 해가 받는 몫을 n 칸으로 반올림한다. (0 이면 부드럽게, 지금까지의 기본)
//   set_band_ramp(문턱, 값, 반 램버트) 는 몫을 문턱으로 갈라 칸마다의 값을 쓴다. 반 램버트면 dot x 0.5 + 0.5 를, 아니면 max(dot, 0) 을 문턱에 댄다. 램프가 있으면 띠 칸 수는 쓰지 않는다. 점광의 받는 몫에도 같은 띠를 건다.
// - 빛 꼴: set_light_model(ToonLightModel { is_energy_normalized }) 가 true 면 바탕색 x 빛을 π 로 나눈다. (램버트 BRDF 의 정석, 빛 세기를 조도로 주는 three.js 의 물리 빛, glTF 빛과 같은 약속)
//   그때 정반사는 정규화한 블린-퐁 (exponent + 8) / 8π 를 곱한다. 기본 false 면 지금까지처럼 바탕색 x 빛이다.
// - 색은 모두 선형 색이다. (HDR 이면 1 을 넘어도 된다)
// - 고치면 판 번호(get_version)가 오른다. 렌더러는 묶음 번호(get_id)와 판 번호가 같으면 다시 올리지 않는다.
//==============================================================================
use std::sync::atomic::{AtomicU32, Ordering};

// 한 그리기에 받는 점광 수의 상한. (셰이더의 유니폼 배열 크기)
pub const TOON_MAXIMUM_POINT_LIGHTS: usize = 8;

// 띠 램프 칸 수의 상한. (문턱은 하나 적다)
pub const TOON_MAXIMUM_BAND_STEPS: usize = 8;

// 빛 묶음마다 다른 번호를 주는 셈. (0 은 쓰지 않는다)
static NEXT_LIGHT_SET_ID: AtomicU32 = AtomicU32::new(1);

//==============================================================================
// 점광 하나. (position 월드 자리, color 선형 색, intensity 색에 곱할 세기, range 0 이면 끝없음, decay 거리 감쇠 지수, range_exponent 창 꼴 지수, wrap 감싸기)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToonPointLight {
	pub position: [f32; 3],
	pub color: [f32; 3],
	pub intensity: f32,
	pub range: f32,
	pub decay: f32,
	pub range_exponent: f32,
	pub wrap: f32,
}

impl Default for ToonPointLight {
	fn default() -> Self {
		ToonPointLight { position: [0.0; 3], color: [1.0; 3], intensity: 1.0, range: 0.0, decay: 2.0, range_exponent: 4.0, wrap: 0.0 }
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
// 띠 램프. (문턱은 값보다 하나 적다, 값이 비면 꺼짐)
//==============================================================================
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ToonBandRamp {
	pub thresholds: Vec<f32>,
	pub values: Vec<f32>,
	pub is_half_lambert: bool,
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
	rim_color: [f32; 3],
	rim_power: f32,
	specular_strength: f32,
	specular_exponent: f32,
	point_light_list: Vec<ToonPointLight>,
	point_light_limit: usize,
	band_count: u32,
	band_ramp: ToonBandRamp,
	light_model: ToonLightModel,
}

impl Default for ToonLightSet {
	fn default() -> Self {
		ToonLightSet::new()
	}
}

//==============================================================================
// 복제. (값은 같고 번호는 새로, 렌더러가 두 묶음을 같은 것으로 보지 않게)
//==============================================================================
impl Clone for ToonLightSet {
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
			rim_color: self.rim_color,
			rim_power: self.rim_power,
			specular_strength: self.specular_strength,
			specular_exponent: self.specular_exponent,
			point_light_list: self.point_light_list.clone(),
			point_light_limit: self.point_light_limit,
			band_count: self.band_count,
			band_ramp: self.band_ramp.clone(),
			light_model: self.light_model,
		}
	}
}

impl ToonLightSet {
	//==============================================================================
	// 생성. (ToonRenderer 의 기본 빛과 같은 값)
	//==============================================================================
	pub fn new() -> ToonLightSet {
		ToonLightSet {
			id: NEXT_LIGHT_SET_ID.fetch_add(1, Ordering::Relaxed),
			version: 0,
			sun_direction: [-0.4, -1.0, -0.3],
			sun_color: [0.9, 0.85, 0.8],
			sky_color: [0.45, 0.5, 0.6],
			ground_color: [0.2, 0.18, 0.16],
			fill_direction: [0.0, -1.0, 0.0],
			fill_color: [0.0; 3],
			rim_color: [0.0; 3],
			rim_power: 0.0,
			specular_strength: 0.0,
			specular_exponent: 48.0,
			point_light_list: Vec::new(),
			point_light_limit: TOON_MAXIMUM_POINT_LIGHTS,
			band_count: 0,
			band_ramp: ToonBandRamp::default(),
			light_model: ToonLightModel::default(),
		}
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
	// 해와 바탕빛 설정. (해가 비추는 방향, 해의 색, 하늘색, 땅색)
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
	// 채움 빛 설정. (빛이 나아가는 방향과 색, 색이 0 이면 없음)
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
	// 림 빛 설정. (색과 거듭제곱, 거듭제곱이 0 이하면 없음)
	//==============================================================================
	pub fn set_rim_light(&mut self, rim_color: [f32; 3], rim_power: f32) {
		self.rim_color = rim_color;
		self.rim_power = rim_power.max(0.0);
		self.mark_changed();
	}

	//==============================================================================
	// 림 빛 반환. ((색, 거듭제곱))
	//==============================================================================
	pub fn get_rim_light(&self) -> ([f32; 3], f32) {
		(self.rim_color, self.rim_power)
	}

	//==============================================================================
	// 정반사 설정. (세기 0 이면 없음, 원본 지수 기본 48)
	//==============================================================================
	pub fn set_specular(&mut self, specular_strength: f32, specular_exponent: f32) {
		self.specular_strength = specular_strength.max(0.0);
		self.specular_exponent = specular_exponent.max(1.0);
		self.mark_changed();
	}

	//==============================================================================
	// 정반사 반환. ((세기, 지수))
	//==============================================================================
	pub fn get_specular(&self) -> (f32, f32) {
		(self.specular_strength, self.specular_exponent)
	}

	//==============================================================================
	// 점광 설정. (TOON_MAXIMUM_POINT_LIGHTS 를 넘는 것은 버린다, 음수 range, decay, wrap 은 0 으로)
	//==============================================================================
	pub fn set_point_lights(&mut self, point_light_list: &[ToonPointLight]) {
		self.point_light_list = point_light_list.iter().take(TOON_MAXIMUM_POINT_LIGHTS).map(|point_light| {
			ToonPointLight { range: point_light.range.max(0.0), decay: point_light.decay.max(0.0), range_exponent: point_light.range_exponent.max(0.0001), wrap: point_light.wrap.max(0.0), ..*point_light }
		}).collect();
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
	// 셀 셰이딩 칸 수 설정. (0 이면 부드럽게)
	//==============================================================================
	pub fn set_band_count(&mut self, band_count: u32) {
		self.band_count = band_count;
		self.mark_changed();
	}

	//==============================================================================
	// 셀 셰이딩 칸 수 반환.
	//==============================================================================
	pub fn get_band_count(&self) -> u32 {
		self.band_count
	}

	//==============================================================================
	// 띠 램프 설정. (문턱은 값보다 하나 적다, 값은 TOON_MAXIMUM_BAND_STEPS 까지, 값이 비면 끈다, 모자란 문턱은 아주 큰 수)
	//==============================================================================
	pub fn set_band_ramp(&mut self, thresholds: &[f32], values: &[f32], is_half_lambert: bool) {
		let value_count = values.len().min(TOON_MAXIMUM_BAND_STEPS);
		let threshold_count = value_count.saturating_sub(1);
		let mut threshold_list: Vec<f32> = thresholds.iter().take(threshold_count).copied().collect();
		while threshold_list.len() < threshold_count {
			threshold_list.push(1e9);
		}
		self.band_ramp = ToonBandRamp { thresholds: threshold_list, values: values[..value_count].to_vec(), is_half_lambert };
		self.mark_changed();
	}

	//==============================================================================
	// 띠 램프 끄기.
	//==============================================================================
	pub fn clear_band_ramp(&mut self) {
		self.band_ramp = ToonBandRamp::default();
		self.mark_changed();
	}

	//==============================================================================
	// 띠 램프 반환. (값이 비면 꺼짐)
	//==============================================================================
	pub fn get_band_ramp(&self) -> &ToonBandRamp {
		&self.band_ramp
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
	fn band_ramp_and_point_lights_follow_limits() {
		let mut light_set = ToonLightSet::new();
		let first_version = light_set.get_version();
		light_set.set_band_ramp(&[0.25, 0.5, 0.75], &[0.3725, 0.6471, 0.8627, 1.0], true);
		assert_eq!(light_set.get_band_ramp().thresholds.len(), 3);
		assert!(light_set.get_version() > first_version);
		light_set.set_band_ramp(&[0.5], &[0.2, 0.6, 1.0], false);
		assert_eq!(light_set.get_band_ramp().thresholds, vec![0.5, 1e9]);
		let point_light_list = vec![ToonPointLight { range: -1.0, ..ToonPointLight::default() }; 10];
		light_set.set_point_lights(&point_light_list);
		assert_eq!(light_set.get_point_lights().len(), TOON_MAXIMUM_POINT_LIGHTS);
		assert_eq!(light_set.get_point_lights()[0].range, 0.0);
		light_set.set_point_light_limit(4);
		assert_eq!(light_set.get_point_light_count(), 4);
		assert_ne!(ToonLightSet::new().get_id(), light_set.get_id());
	}
}
