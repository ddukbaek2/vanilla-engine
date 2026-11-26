//==============================================================================
//! PannerNode 셈. (크로미엄 panner_handler.cc, distance_effect.cc, cone_effect.cc, equal_power_panner.cc 와 ui/gfx 의 Vector3dF 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. (experimental::spatialaudio 가 이 셈을 내보낸다)
//! - 데스크탑 믹서의 PannerNode(panner.rs)와 게임 코드가 같은 셈을 쓴다. 웹은 브라우저 PannerNode 가 같은 셈을 한다.
//! - 정밀도는 크로미엄 그대로다. 벡터는 f32 칸이고, 길이 제곱과 외적은 f64 로 셈해 f32 로, 내적과 사이 각은 f32 로, 거리와 원뿔 이득은 f64 로 셈한다.
//!   (크로미엄은 -ffp-contract=off 로 빌드해 곱셈과 덧셈을 합치지 않는다. 러스트도 합치지 않는다)
//! - 좌표는 WebAudio 와 같은 오른손 좌표다. 리스너 기본은 자리 (0, 0, 0), 앞 (0, 0, -1), 위 (0, 1, 0)이다.
//==============================================================================
use crate::base::vector3::Vector3;

//==============================================================================
/// 좌우 나누는 방식. (WebAudio PanningModelType)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PanningModel {
	/// 등력 좌우. (WebAudio "equalpower", 기본) 방위각만 쓰고 높이는 쓰지 않는다.
	#[default]
	EqualPower,
	/// 머리 전달 함수. (WebAudio "HRTF") 웹은 브라우저가 IRCAM 자료로 셈하고, 데스크탑은 아직 등력 좌우로 셈한다.
	Hrtf,
}

//==============================================================================
/// 거리에 따른 줄임 방식. (WebAudio DistanceModelType)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DistanceModel {
	/// 선형. (1 - rolloff x (거리 - 기준) / (최대 - 기준), 거리는 기준과 최대 사이로 자르고 rolloff 는 0 ~ 1 로 자른다)
	Linear,
	/// 역수. (기준 / (기준 + rolloff x (거리 - 기준)), 기본)
	#[default]
	Inverse,
	/// 지수. ((거리 / 기준) ^ -rolloff)
	Exponential,
}

//==============================================================================
/// PannerNode 의 거리와 원뿔, 좌우 방식 설정. (WebAudio PannerNode 의 속성들, 기본값도 같다)
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{DistanceModel, PannerSettings};
///
/// let mut settings = PannerSettings::default();
/// settings.distance_model = DistanceModel::Linear;
/// settings.maximum_distance = 20.0;
/// assert_eq!(settings.reference_distance, 1.0);
/// assert!(settings.is_valid());
/// ```
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PannerSettings {
	/// 좌우 나누는 방식. (기본 등력)
	pub panning_model: PanningModel,
	/// 거리 줄임 방식. (기본 역수)
	pub distance_model: DistanceModel,
	/// 기준 거리. (refDistance, 0 이상, 기본 1)
	pub reference_distance: f64,
	/// 최대 거리. (maxDistance, 0 보다 큼, 기본 10000)
	pub maximum_distance: f64,
	/// 줄어드는 빠르기. (rolloffFactor, 0 이상, 기본 1)
	pub rolloff_factor: f64,
	/// 원뿔 안쪽 각. (도, 줄지 않는 각 전체, 기본 360)
	pub cone_inner_angle: f64,
	/// 원뿔 바깥쪽 각. (도, 기본 360)
	pub cone_outer_angle: f64,
	/// 원뿔 바깥의 이득. (0 ~ 1, 기본 0)
	pub cone_outer_gain: f64,
}

impl Default for PannerSettings {
	//==============================================================================
	/// WebAudio 기본값. (등력, 역수, 1, 10000, 1, 360, 360, 0)
	//==============================================================================
	fn default() -> PannerSettings {
		PannerSettings {
			panning_model: PanningModel::EqualPower,
			distance_model: DistanceModel::Inverse,
			reference_distance: 1.0,
			maximum_distance: 10000.0,
			rolloff_factor: 1.0,
			cone_inner_angle: 360.0,
			cone_outer_angle: 360.0,
			cone_outer_gain: 0.0,
		}
	}
}

impl PannerSettings {
	//==============================================================================
	/// WebAudio 가 받는 값인지. (기준 거리 0 이상, 최대 거리 0 보다 큼, rolloff 0 이상, 원뿔 바깥 이득 0 ~ 1, 모두 유한)
	/// - WebAudio 는 이 밖의 값에 RangeError 를 던진다. 엔진은 그 값을 받지 않고 앞 값을 둔다.
	///
	/// # 돌려줌
	/// 받는 값이면 true.
	//==============================================================================
	pub fn is_valid(&self) -> bool {
		let is_finite = self.reference_distance.is_finite() && self.maximum_distance.is_finite() && self.rolloff_factor.is_finite() && self.cone_inner_angle.is_finite() && self.cone_outer_angle.is_finite() && self.cone_outer_gain.is_finite();
		is_finite && self.reference_distance >= 0.0 && self.maximum_distance > 0.0 && self.rolloff_factor >= 0.0 && (0.0..=1.0).contains(&self.cone_outer_gain)
	}
}

// 길이가 이보다 작은 벡터는 0 으로 친다. (gfx kEpsilon)
const VECTOR_EPSILON: f64 = 1.0e-6;
// 라디안을 도로. (base::RadToDeg 의 float 판, 180 / pi_v<float>)
const RADIAN_TO_DEGREE_F32: f32 = 180.0f32 / std::f32::consts::PI;

pub(crate) type Vector3F = [f32; 3];

//==============================================================================
/// Vector3 를 f32 칸 셋으로.
///
/// # 인자
/// - `vector`: 벡터.
///
/// # 돌려줌
/// [x, y, z].
//==============================================================================
pub(crate) fn to_vector3f(vector: Vector3) -> Vector3F {
	[vector.x, vector.y, vector.z]
}

//==============================================================================
/// 빼기. (f32)
///
/// # 인자
/// - `left`: 왼쪽.
/// - `right`: 오른쪽.
///
/// # 돌려줌
/// left - right.
//==============================================================================
fn subtract(left: Vector3F, right: Vector3F) -> Vector3F {
	[left[0] - right[0], left[1] - right[1], left[2] - right[2]]
}

//==============================================================================
/// 길이 제곱. (f64)
///
/// # 인자
/// - `vector`: 벡터.
///
/// # 돌려줌
/// 길이 제곱.
//==============================================================================
fn length_squared(vector: Vector3F) -> f64 {
	vector[0] as f64 * vector[0] as f64 + vector[1] as f64 * vector[1] as f64 + vector[2] as f64 * vector[2] as f64
}

//==============================================================================
/// 길이. (f64 로 셈해 f32 로)
///
/// # 인자
/// - `vector`: 벡터.
///
/// # 돌려줌
/// 길이.
//==============================================================================
fn length(vector: Vector3F) -> f32 {
	length_squared(vector).sqrt() as f32
}

//==============================================================================
// 단위 벡터. (gfx Vector3dF::GetNormalized, 길이가 너무 작으면 그대로 두고 거짓)
//==============================================================================
fn get_normalized(vector: Vector3F) -> (Vector3F, bool) {
	let squared = length_squared(vector);
	if squared < VECTOR_EPSILON * VECTOR_EPSILON {
		return (vector, false);
	}
	let inverse_scale = squared.sqrt() as f32;
	([vector[0] / inverse_scale, vector[1] / inverse_scale, vector[2] / inverse_scale], true)
}

//==============================================================================
// 외적. (gfx Vector3dF::Cross, f64 로 셈해 f32 로)
//==============================================================================
fn cross(left: Vector3F, right: Vector3F) -> Vector3F {
	let (left_x, left_y, left_z) = (left[0] as f64, left[1] as f64, left[2] as f64);
	[(left_y * right[2] as f64 - left_z * right[1] as f64) as f32, (left_z * right[0] as f64 - left_x * right[2] as f64) as f32, (left_x * right[1] as f64 - left_y * right[0] as f64) as f32]
}

//==============================================================================
/// 내적. (f32)
///
/// # 인자
/// - `left`: 왼쪽.
/// - `right`: 오른쪽.
///
/// # 돌려줌
/// 내적.
//==============================================================================
fn dot(left: Vector3F, right: Vector3F) -> f32 {
	left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

//==============================================================================
/// 배율 곱하기. (f32)
///
/// # 인자
/// - `vector`: 벡터.
/// - `scale`: 배율.
///
/// # 돌려줌
/// 곱한 벡터.
//==============================================================================
fn scale(vector: Vector3F, scale: f32) -> Vector3F {
	[vector[0] * scale, vector[1] * scale, vector[2] * scale]
}

//==============================================================================
/// 세 칸이 모두 0 인지.
///
/// # 인자
/// - `vector`: 벡터.
///
/// # 돌려줌
/// 모두 0 이면 true.
//==============================================================================
fn is_zero(vector: Vector3F) -> bool {
	vector[0] == 0.0 && vector[1] == 0.0 && vector[2] == 0.0
}

//==============================================================================
// 두 벡터 사이 각. (도, gfx AngleBetweenVectorsInDegrees, f32 로 셈한다)
//==============================================================================
fn angle_between_vectors_in_degrees(base: Vector3F, other: Vector3F) -> f32 {
	let cosine = dot(base, other) / length(base) / length(other);
	cosine.min(1.0).max(-1.0).acos() * RADIAN_TO_DEGREE_F32
}

//==============================================================================
// 값 자르기. (wtf ClampTo: 최대 이상이면 최대, 최소 이하면 최소, NaN 은 그대로)
//==============================================================================
fn clamp_to(value: f64, minimum: f64, maximum: f64) -> f64 {
	if value >= maximum {
		return maximum;
	}
	if value <= minimum {
		minimum
	}
	else {
		value
	}
}

//==============================================================================
/// 유한하지 않으면 0.
///
/// # 인자
/// - `value`: 값.
///
/// # 돌려줌
/// 유한한 값.
//==============================================================================
fn ensure_finite(value: f64) -> f64 {
	if value.is_finite() {
		value
	}
	else {
		0.0
	}
}

//==============================================================================
/// 거리 이득. (크로미엄 DistanceEffect::Gain, WebAudio 의 linear, inverse, exponential 식)
/// - linear: 기준과 최대 중 작은 것과 큰 것으로 거리를 자르고, 둘이 같으면 1 - rolloff, 아니면 1 - clamp(rolloff, 0, 1) x (거리 - 작은 것) / (큰 것 - 작은 것).
/// - inverse: 기준이 0 이면 0, 거리를 기준 이상으로 자르고 기준 / (기준 + max(rolloff, 0) x (거리 - 기준)).
/// - exponential: 기준이 0 이면 0, 거리를 기준 이상으로 자르고 (거리 / 기준) ^ -max(rolloff, 0).
///
/// # 인자
/// - `distance_model`: 거리 모델.
/// - `distance`: 소리와 리스너 사이 거리.
/// - `reference_distance`: 기준 거리.
/// - `maximum_distance`: 최대 거리.
/// - `rolloff_factor`: 줄어드는 빠르기.
///
/// # 돌려줌
/// 거리 이득.
///
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{calculate_distance_gain, DistanceModel};
///
/// assert_eq!(calculate_distance_gain(DistanceModel::Inverse, 4.0, 1.0, 10000.0, 1.0), 0.25);
/// assert_eq!(calculate_distance_gain(DistanceModel::Linear, 6.0, 2.0, 10.0, 1.0), 0.5);
/// assert_eq!(calculate_distance_gain(DistanceModel::Exponential, 0.5, 1.0, 10000.0, 2.0), 1.0);
/// ```
//==============================================================================
pub fn calculate_distance_gain(distance_model: DistanceModel, distance: f64, reference_distance: f64, maximum_distance: f64, rolloff_factor: f64) -> f64 {
	match distance_model {
		DistanceModel::Linear => {
			let minimum_distance = reference_distance.min(maximum_distance);
			let largest_distance = reference_distance.max(maximum_distance);
			let clamped_distance = clamp_to(distance, minimum_distance, largest_distance);
			if minimum_distance == largest_distance {
				return 1.0 - rolloff_factor;
			}
			1.0 - clamp_to(rolloff_factor, 0.0, 1.0) * (clamped_distance - minimum_distance) / (largest_distance - minimum_distance)
		}
		DistanceModel::Inverse => {
			if reference_distance == 0.0 {
				return 0.0;
			}
			let clamped_distance = clamp_to(distance, reference_distance, f64::MAX);
			reference_distance / (reference_distance + clamp_to(rolloff_factor, 0.0, f64::MAX) * (clamped_distance - reference_distance))
		}
		DistanceModel::Exponential => {
			if reference_distance == 0.0 {
				return 0.0;
			}
			let clamped_distance = clamp_to(distance, reference_distance, f64::MAX);
			(clamped_distance / reference_distance).powf(-clamp_to(rolloff_factor, 0.0, f64::MAX))
		}
	}
}

//==============================================================================
/// 원뿔 이득. (크로미엄 ConeEffect::Gain, OpenAL 명세의 원뿔)
/// - 소리의 방향이 0 이거나 안쪽, 바깥쪽 각이 모두 360 이면 1. 소리 → 리스너 벡터와 방향 사이 각이 안쪽 각의 반 이하면 1,
///   바깥쪽 각의 반 이상이면 바깥 이득, 그 사이는 선형으로 섞는다.
///
/// # 인자
/// - `source_position`: 소리 자리.
/// - `source_orientation`: 소리가 보는 방향.
/// - `listener_position`: 리스너 자리.
/// - `cone_inner_angle`: 안쪽 각. (도, 각 전체)
/// - `cone_outer_angle`: 바깥쪽 각. (도, 각 전체)
/// - `cone_outer_gain`: 바깥 이득. (0 ~ 1)
///
/// # 돌려줌
/// 원뿔 이득.
///
/// # 예제
/// 소리 뒤에 선 리스너의 원뿔 이득
/// ```
/// use vanilla::experimental::spatialaudio::calculate_cone_gain;
/// use vanilla::Vector3;
///
/// let gain = calculate_cone_gain(Vector3::create(0.0, 0.0, 0.0), Vector3::create(1.0, 0.0, 0.0), Vector3::create(-5.0, 0.0, 0.0), 60.0, 180.0, 0.25);
/// assert_eq!(gain, 0.25);
/// ```
//==============================================================================
pub fn calculate_cone_gain(source_position: Vector3, source_orientation: Vector3, listener_position: Vector3, cone_inner_angle: f64, cone_outer_angle: f64, cone_outer_gain: f64) -> f64 {
	calculate_cone_gain_f(to_vector3f(source_position), to_vector3f(source_orientation), to_vector3f(listener_position), cone_inner_angle, cone_outer_angle, cone_outer_gain)
}

//==============================================================================
/// 원뿔 이득. (f32 칸 셋을 받는 안쪽 판)
///
/// # 인자
/// - `source_position`: 소리 자리.
/// - `source_orientation`: 소리가 보는 방향.
/// - `listener_position`: 리스너 자리.
/// - `cone_inner_angle`: 안쪽 각.
/// - `cone_outer_angle`: 바깥쪽 각.
/// - `cone_outer_gain`: 바깥 이득.
///
/// # 돌려줌
/// 원뿔 이득.
//==============================================================================
pub(crate) fn calculate_cone_gain_f(source_position: Vector3F, source_orientation: Vector3F, listener_position: Vector3F, cone_inner_angle: f64, cone_outer_angle: f64, cone_outer_gain: f64) -> f64 {
	if is_zero(source_orientation) || (cone_inner_angle == 360.0 && cone_outer_angle == 360.0) {
		return 1.0;
	}
	let source_to_listener = subtract(listener_position, source_position);
	let angle = angle_between_vectors_in_degrees(source_to_listener, source_orientation) as f64;
	let absolute_angle = angle.abs();
	let absolute_inner_angle = cone_inner_angle.abs() / 2.0;
	let absolute_outer_angle = cone_outer_angle.abs() / 2.0;
	if absolute_angle <= absolute_inner_angle {
		1.0
	}
	else if absolute_angle >= absolute_outer_angle {
		cone_outer_gain
	}
	else {
		let ratio = (absolute_angle - absolute_inner_angle) / (absolute_outer_angle - absolute_inner_angle);
		(1.0 - ratio) + cone_outer_gain * ratio
	}
}

//==============================================================================
/// 방위각과 높이. (도, 크로미엄 PannerHandler::CalculateAzimuthElevation)
/// - 방위각은 리스너 앞이 0, 오른쪽이 90, 왼쪽이 -90, 뒤는 180 쪽이다. (-180 ~ 270 사이 값이 나올 수 있고 좌우 셈이 접는다)
/// - 소리와 리스너가 같은 자리면 (0, 0).
///
/// # 인자
/// - `source_position`: 소리 자리.
/// - `listener_position`: 리스너 자리.
/// - `listener_forward`: 리스너 앞.
/// - `listener_up`: 리스너 위.
///
/// # 돌려줌
/// (방위각, 높이). (도)
///
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::calculate_azimuth_elevation;
/// use vanilla::Vector3;
///
/// let (azimuth, elevation) = calculate_azimuth_elevation(Vector3::create(3.0, 0.0, 0.0), Vector3::create(0.0, 0.0, 0.0), Vector3::create(0.0, 0.0, -1.0), Vector3::create(0.0, 1.0, 0.0));
/// assert!((azimuth - 90.0).abs() < 1e-4);
/// assert!(elevation.abs() < 1e-4);
/// ```
//==============================================================================
pub fn calculate_azimuth_elevation(source_position: Vector3, listener_position: Vector3, listener_forward: Vector3, listener_up: Vector3) -> (f64, f64) {
	calculate_azimuth_elevation_f(to_vector3f(source_position), to_vector3f(listener_position), to_vector3f(listener_forward), to_vector3f(listener_up))
}

//==============================================================================
/// 방위각과 높이. (f32 칸 셋을 받는 안쪽 판)
///
/// # 인자
/// - `source_position`: 소리 자리.
/// - `listener_position`: 리스너 자리.
/// - `listener_forward`: 리스너 앞.
/// - `listener_up`: 리스너 위.
///
/// # 돌려줌
/// (방위각, 높이). (도)
//==============================================================================
pub(crate) fn calculate_azimuth_elevation_f(source_position: Vector3F, listener_position: Vector3F, listener_forward: Vector3F, listener_up: Vector3F) -> (f64, f64) {
	let (source_listener, is_normalized) = get_normalized(subtract(source_position, listener_position));
	if !is_normalized {
		return (0.0, 0.0);
	}
	let (listener_right, _) = get_normalized(cross(listener_forward, listener_up));
	let (listener_forward_normalized, _) = get_normalized(listener_forward);
	let up = cross(listener_right, listener_forward_normalized);
	let up_projection = dot(source_listener, up);
	let (projected_source, _) = get_normalized(subtract(source_listener, scale(up, up_projection)));
	// acos 와 RadToDeg 는 float 판이다. (ClampTo<float> 의 값을 받는다)
	let mut azimuth = ensure_finite((dot(projected_source, listener_right).clamp(-1.0, 1.0).acos() * RADIAN_TO_DEGREE_F32) as f64);
	let front_back = dot(projected_source, listener_forward_normalized) as f64;
	if front_back < 0.0 {
		azimuth = 360.0 - azimuth;
	}
	if (0.0..=270.0).contains(&azimuth) {
		azimuth = 90.0 - azimuth;
	}
	else {
		azimuth = 450.0 - azimuth;
	}
	// 90 - float 은 float 셈이다.
	let mut elevation = ensure_finite((90.0f32 - angle_between_vectors_in_degrees(source_listener, up)) as f64);
	if elevation > 90.0 {
		elevation = 180.0 - elevation;
	}
	else if elevation < -90.0 {
		elevation = -180.0 - elevation;
	}
	(azimuth, elevation)
}

//==============================================================================
/// 거리 이득 x 원뿔 이득. (크로미엄 PannerHandler::CalculateDistanceConeGain, f32 로)
///
/// # 인자
/// - `settings`: 설정.
/// - `source_position`: 소리 자리.
/// - `source_orientation`: 소리가 보는 방향.
/// - `listener_position`: 리스너 자리.
///
/// # 돌려줌
/// 거리 x 원뿔 이득.
///
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{calculate_distance_cone_gain, PannerSettings};
/// use vanilla::Vector3;
///
/// let gain = calculate_distance_cone_gain(&PannerSettings::default(), Vector3::create(0.0, 0.0, -2.0), Vector3::create(1.0, 0.0, 0.0), Vector3::create(0.0, 0.0, 0.0));
/// assert_eq!(gain, 0.5);
/// ```
//==============================================================================
pub fn calculate_distance_cone_gain(settings: &PannerSettings, source_position: Vector3, source_orientation: Vector3, listener_position: Vector3) -> f32 {
	calculate_distance_cone_gain_f(settings, to_vector3f(source_position), to_vector3f(source_orientation), to_vector3f(listener_position))
}

//==============================================================================
/// 거리 x 원뿔 이득. (f32 칸 셋을 받는 안쪽 판)
///
/// # 인자
/// - `settings`: 설정.
/// - `source_position`: 소리 자리.
/// - `source_orientation`: 소리가 보는 방향.
/// - `listener_position`: 리스너 자리.
///
/// # 돌려줌
/// 거리 x 원뿔 이득.
//==============================================================================
pub(crate) fn calculate_distance_cone_gain_f(settings: &PannerSettings, source_position: Vector3F, source_orientation: Vector3F, listener_position: Vector3F) -> f32 {
	let listener_distance = length(subtract(source_position, listener_position)) as f64;
	let distance_gain = calculate_distance_gain(settings.distance_model, listener_distance, settings.reference_distance, settings.maximum_distance, settings.rolloff_factor);
	let cone_gain = calculate_cone_gain_f(source_position, source_orientation, listener_position, settings.cone_inner_angle, settings.cone_outer_angle, settings.cone_outer_gain);
	(distance_gain * cone_gain) as f32
}

//==============================================================================
// 방위각을 -180 ~ 180 으로 자르고 뒤쪽을 앞쪽으로 접는다. (EqualPowerPanner::UpdateDesiredGain 의 앞)
//==============================================================================
pub(crate) fn alias_azimuth(azimuth: f64) -> f64 {
	let clamped_azimuth = clamp_to(azimuth, -180.0, 180.0);
	if clamped_azimuth < -90.0 {
		-180.0 - clamped_azimuth
	}
	else if clamped_azimuth > 90.0 {
		180.0 - clamped_azimuth
	}
	else {
		clamped_azimuth
	}
}

//==============================================================================
/// 등력 좌우 이득. (크로미엄 EqualPowerPanner::UpdateDesiredGain, (왼쪽, 오른쪽))
/// - 방위각을 접은 뒤 모노는 x = (방위각 + 90) / 180, 스테레오는 방위각이 0 이하면 (방위각 + 90) / 90, 아니면 방위각 / 90 으로
///   왼쪽 cos(x π/2), 오른쪽 sin(x π/2).
/// - 스테레오 입력은 방위각이 0 이하면 왼쪽 = 왼쪽 + 오른쪽 x 왼쪽 이득, 오른쪽 = 오른쪽 x 오른쪽 이득이고, 아니면 거꾸로 섞는다.
///
/// # 인자
/// - `azimuth`: 방위각. (도)
/// - `input_channel_count`: 입력 채널 수. (1 이나 2)
///
/// # 돌려줌
/// (왼쪽 이득, 오른쪽 이득).
///
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::calculate_equal_power_gains;
///
/// let (left, right) = calculate_equal_power_gains(0.0, 1);
/// assert!((left - right).abs() < 1e-12);
/// let (left, right) = calculate_equal_power_gains(90.0, 1);
/// assert!(left.abs() < 1e-12 && (right - 1.0).abs() < 1e-12);
/// ```
//==============================================================================
pub fn calculate_equal_power_gains(azimuth: f64, input_channel_count: usize) -> (f64, f64) {
	let azimuth = alias_azimuth(azimuth);
	let desired_pan_position = if input_channel_count == 1 {
		(azimuth + 90.0) / 180.0
	}
	else if azimuth <= 0.0 {
		(azimuth + 90.0) / 90.0
	}
	else {
		azimuth / 90.0
	};
	let radian = std::f64::consts::FRAC_PI_2 * desired_pan_position;
	(radian.cos(), radian.sin())
}

//==============================================================================
// 등력 좌우를 표본 하나에. (EqualPowerPanner::Pan 의 안쪽, 이득은 f64 로 곱해 f32 로)
//==============================================================================
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn pan_equal_power_sample(azimuth: f64, input_channel_count: usize, input_left: f32, input_right: f32) -> (f32, f32) {
	let (gain_left, gain_right) = calculate_equal_power_gains(azimuth, input_channel_count);
	if input_channel_count == 1 {
		return ((input_left as f64 * gain_left) as f32, (input_left as f64 * gain_right) as f32);
	}
	if alias_azimuth(azimuth) <= 0.0 {
		((input_left as f64 + input_right as f64 * gain_left) as f32, (input_right as f64 * gain_right) as f32)
	}
	else {
		((input_left as f64 * gain_left) as f32, (input_right as f64 + input_left as f64 * gain_right) as f32)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 거리 모델: WebAudio 식과 모서리 값. (기준 0, 기준과 최대가 같음, rolloff 자르기)
	//==============================================================================
	#[test]
	fn distance_models_follow_webaudio() {
		assert_eq!(calculate_distance_gain(DistanceModel::Inverse, 0.5, 1.0, 10000.0, 1.0), 1.0);
		assert_eq!(calculate_distance_gain(DistanceModel::Inverse, 3.0, 1.0, 10000.0, 0.5), 0.5);
		assert_eq!(calculate_distance_gain(DistanceModel::Inverse, 3.0, 0.0, 10000.0, 1.0), 0.0);
		assert_eq!(calculate_distance_gain(DistanceModel::Linear, 100.0, 2.0, 10.0, 1.0), 0.0);
		assert_eq!(calculate_distance_gain(DistanceModel::Linear, 6.0, 2.0, 10.0, 3.0), 0.5);
		assert_eq!(calculate_distance_gain(DistanceModel::Linear, 6.0, 5.0, 5.0, 0.25), 0.75);
		assert_eq!(calculate_distance_gain(DistanceModel::Linear, 6.0, 10.0, 2.0, 1.0), 0.5);
		assert_eq!(calculate_distance_gain(DistanceModel::Exponential, 4.0, 2.0, 10000.0, 1.0), 0.5);
		assert_eq!(calculate_distance_gain(DistanceModel::Exponential, 4.0, 0.0, 10000.0, 1.0), 0.0);
		assert_eq!(calculate_distance_gain(DistanceModel::Exponential, 4.0, 2.0, 10000.0, -1.0), 1.0);
	}

	//==============================================================================
	// 원뿔: 안쪽, 사이(선형), 바깥, 방향 0, 360 두 개.
	//==============================================================================
	#[test]
	fn cone_gain_follows_openal() {
		let source = [0.0, 0.0, 0.0];
		let orientation = [1.0, 0.0, 0.0];
		assert_eq!(calculate_cone_gain_f(source, orientation, [5.0, 0.0, 0.0], 60.0, 180.0, 0.2), 1.0);
		assert_eq!(calculate_cone_gain_f(source, orientation, [-5.0, 0.0, 0.0], 60.0, 180.0, 0.2), 0.2);
		let middle_gain = calculate_cone_gain_f(source, orientation, [0.0, 5.0, 0.0], 60.0, 300.0, 0.2);
		assert!((middle_gain - (1.0 - 0.5 + 0.2 * 0.5)).abs() < 1e-6);
		assert_eq!(calculate_cone_gain_f(source, [0.0, 0.0, 0.0], [-5.0, 0.0, 0.0], 60.0, 180.0, 0.2), 1.0);
		assert_eq!(calculate_cone_gain_f(source, orientation, [-5.0, 0.0, 0.0], 360.0, 360.0, 0.2), 1.0);
	}

	//==============================================================================
	// 방위각: 앞, 오른쪽, 왼쪽, 뒤, 같은 자리, 리스너가 돌았을 때.
	//==============================================================================
	#[test]
	fn azimuth_follows_listener_axes() {
		let listener = [0.0, 0.0, 0.0];
		let forward = [0.0, 0.0, -1.0];
		let up = [0.0, 1.0, 0.0];
		let (azimuth, _) = calculate_azimuth_elevation_f([0.0, 0.0, -4.0], listener, forward, up);
		assert!(azimuth.abs() < 1e-4);
		let (azimuth, _) = calculate_azimuth_elevation_f([-2.0, 0.0, 0.0], listener, forward, up);
		assert!((azimuth + 90.0).abs() < 1e-4);
		let (azimuth, _) = calculate_azimuth_elevation_f([0.0, 0.0, 3.0], listener, forward, up);
		assert!((azimuth - 180.0).abs() < 1e-4 || (azimuth + 180.0).abs() < 1e-4);
		assert_eq!(calculate_azimuth_elevation_f(listener, listener, forward, up), (0.0, 0.0));
		let (azimuth, elevation) = calculate_azimuth_elevation_f([0.0, 5.0, 0.0], listener, forward, up);
		assert!(azimuth.abs() < 1e-4 && (elevation - 90.0).abs() < 1e-4);
		// 리스너가 +x 를 보면 -z 쪽 소리는 왼쪽이다.
		let (azimuth, _) = calculate_azimuth_elevation_f([0.0, 0.0, -4.0], listener, [1.0, 0.0, 0.0], up);
		assert!((azimuth + 90.0).abs() < 1e-4);
	}

	//==============================================================================
	// 등력 좌우: 접기와 스테레오 섞기.
	//==============================================================================
	#[test]
	fn equal_power_pan_folds_and_mixes() {
		assert_eq!(alias_azimuth(135.0), 45.0);
		assert_eq!(alias_azimuth(-135.0), -45.0);
		assert_eq!(alias_azimuth(400.0), 0.0);
		let (left, right) = pan_equal_power_sample(-90.0, 2, 0.5, 0.25);
		assert_eq!(left, 0.75);
		assert!(right.abs() < 1e-7);
		let (left, right) = pan_equal_power_sample(90.0, 2, 0.5, 0.25);
		assert!(left.abs() < 1e-7);
		assert_eq!(right, 0.75);
	}
}
