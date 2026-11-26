//==============================================================================
//! 데스크탑 믹서의 PannerNode 와 AudioListener. (크로미엄 panner_handler.cc, audio_listener_handler.cc 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. (experimental::spatialaudio 의 SpatialAudioPlayer, SpatialAudioListener 가 쓴다)
//! - 자리와 방향은 a-rate AudioParam 이다. (audioparam.rs) 퀀텀마다 PannerHandler::ProcessIfNecessary 와 같은 차례로 셈한다.
//!   입력이 조용하고 침묵을 넘길 때(마지막 소리 뒤 퀀텀부터)는 AudioParam 만 흘리고(ProcessOnlyAudioParams),
//!   자리나 리스너에 이 퀀텀 자동화가 있으면 표본마다(ProcessSampleAccurateValues), 아니면 바뀐 때만 방위각과 이득을 다시 셈한다.
//! - 리스너는 렌더 퀀텀마다 그리기 전에 UpdateState(값을 읽어 바뀌었는지)를 한다. (OfflineAudioContext::HandlePreRenderTasks)
//! - 좌우는 등력(equal_power_panner.cc)이다. HRTF 를 고르면 데스크탑은 아직 등력으로 셈한다. (웹은 브라우저 HRTF)
//! - 크로미엄처럼 거리 모델만 바꾸면 이득을 다시 셈하지 않는다. (SetDistanceModel 은 dirty 를 걸지 않는다) 기준, 최대, rolloff, 원뿔 값은 바뀌면 건다.
//==============================================================================
use super::audioparam::{AudioParam, ParamAutomation};
use super::mixer::RENDER_QUANTUM_FRAMES;
use super::pannermath::{alias_azimuth, calculate_azimuth_elevation_f, calculate_distance_cone_gain_f, calculate_equal_power_gains, PannerSettings, Vector3F};
use super::{ListenerParam, PannerParam};

type Quantum = [[f32; RENDER_QUANTUM_FRAMES]; 2];

/// 자리와 방향 AudioParam 의 범위. (f32 전체)
fn create_unbounded_param(default_value: f32) -> AudioParam {
	AudioParam::new(default_value, f32::MIN, f32::MAX)
}

//==============================================================================
/// 리스너. (AudioListenerHandler: 자리, 앞, 위의 AudioParam 아홉)
//==============================================================================
pub(crate) struct ListenerState {
	param_list: [AudioParam; 9],
	last_position: Vector3F,
	last_forward: Vector3F,
	last_up: Vector3F,
	is_listener_dirty: bool,
	value_list: [[f32; RENDER_QUANTUM_FRAMES]; 9],
	last_update_frame: Option<u64>,
}

impl ListenerState {
	//==============================================================================
	/// 생성. (자리 (0, 0, 0), 앞 (0, 0, -1), 위 (0, 1, 0))
	//==============================================================================
	pub(crate) fn new() -> ListenerState {
		ListenerState {
			param_list: [
				create_unbounded_param(0.0),
				create_unbounded_param(0.0),
				create_unbounded_param(0.0),
				create_unbounded_param(0.0),
				create_unbounded_param(0.0),
				create_unbounded_param(-1.0),
				create_unbounded_param(0.0),
				create_unbounded_param(1.0),
				create_unbounded_param(0.0),
			],
			last_position: [0.0, 0.0, 0.0],
			last_forward: [0.0, 0.0, -1.0],
			last_up: [0.0, 1.0, 0.0],
			is_listener_dirty: false,
			value_list: [[0.0; RENDER_QUANTUM_FRAMES]; 9],
			last_update_frame: None,
		}
	}

	//==============================================================================
	/// 자동화 받기.
	//==============================================================================
	pub(crate) fn apply_automation(&mut self, param: ListenerParam, automation: ParamAutomation, current_frame: u64, sample_rate: f64) {
		self.param_list[param as usize].apply_automation(automation, current_frame, sample_rate);
	}

	/// Value() 셋. (k-rate 값, 내재 값을 고친다)
	fn read_vector(&mut self, first_index: usize, current_frame: u64, sample_rate: f64) -> Vector3F {
		let mut vector = [0.0f32; 3];
		for (offset, value) in vector.iter_mut().enumerate() {
			*value = self.param_list[first_index + offset].calculate_quantum_value(current_frame, sample_rate, RENDER_QUANTUM_FRAMES);
		}
		vector
	}

	//==============================================================================
	/// 자리. (Value() 셋)
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 자리.
	//==============================================================================
	fn get_position(&mut self, current_frame: u64, sample_rate: f64) -> Vector3F {
		self.read_vector(0, current_frame, sample_rate)
	}

	//==============================================================================
	/// 앞. (Value() 셋)
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 앞.
	//==============================================================================
	fn get_orientation(&mut self, current_frame: u64, sample_rate: f64) -> Vector3F {
		self.read_vector(3, current_frame, sample_rate)
	}

	//==============================================================================
	/// 위. (Value() 셋)
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 위.
	//==============================================================================
	fn get_up_vector(&mut self, current_frame: u64, sample_rate: f64) -> Vector3F {
		self.read_vector(6, current_frame, sample_rate)
	}

	//==============================================================================
	/// 그리기 전 상태. (UpdateState: 자리, 앞, 위가 앞 퀀텀과 다르면 dirty)
	//==============================================================================
	pub(crate) fn update_state(&mut self, current_frame: u64, sample_rate: f64) {
		let current_position = self.get_position(current_frame, sample_rate);
		let current_forward = self.get_orientation(current_frame, sample_rate);
		let current_up = self.get_up_vector(current_frame, sample_rate);
		self.is_listener_dirty = current_position != self.last_position || current_forward != self.last_forward || current_up != self.last_up;
		if self.is_listener_dirty {
			self.last_position = current_position;
			self.last_forward = current_forward;
			self.last_up = current_up;
		}
	}

	//==============================================================================
	/// 이 퀀텀에 표본마다 셈할 자동화가 있는지.
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 하나라도 있으면 true.
	//==============================================================================
	fn has_sample_accurate_values(&self, current_frame: u64, sample_rate: f64) -> bool {
		self.param_list.iter().any(|param| param.has_sample_accurate_values(current_frame, sample_rate, RENDER_QUANTUM_FRAMES))
	}

	//==============================================================================
	/// 표본마다의 값. (UpdateValuesIfNeeded, 퀀텀마다 한 번)
	//==============================================================================
	fn update_values_if_needed(&mut self, current_frame: u64, sample_rate: f64) {
		if self.last_update_frame == Some(current_frame) {
			return;
		}
		self.last_update_frame = Some(current_frame);
		for (param, value_list) in self.param_list.iter_mut().zip(self.value_list.iter_mut()) {
			param.calculate_sample_accurate_values(value_list, current_frame, sample_rate);
		}
	}
}

//==============================================================================
/// PannerNode 하나. (PannerHandler)
//==============================================================================
pub(crate) struct PannerNode {
	param_list: [AudioParam; 6],
	settings: PannerSettings,
	last_position: Vector3F,
	last_orientation: Vector3F,
	cached_azimuth: f64,
	is_azimuth_elevation_dirty: bool,
	cached_distance_cone_gain: f32,
	is_distance_cone_gain_dirty: bool,
	/// 마지막으로 소리가 난 퀀텀의 끝 시각. (초, PropagatesSilence)
	last_non_silent_time: f64,
	value_list: [[f32; RENDER_QUANTUM_FRAMES]; 6],
	azimuth_value_list: [f64; RENDER_QUANTUM_FRAMES],
	total_gain_value_list: [f32; RENDER_QUANTUM_FRAMES],
}

impl PannerNode {
	//==============================================================================
	/// 생성. (자리 (0, 0, 0), 방향 (1, 0, 0))
	//==============================================================================
	pub(crate) fn new(settings: PannerSettings) -> PannerNode {
		PannerNode {
			param_list: [create_unbounded_param(0.0), create_unbounded_param(0.0), create_unbounded_param(0.0), create_unbounded_param(1.0), create_unbounded_param(0.0), create_unbounded_param(0.0)],
			settings,
			last_position: [0.0, 0.0, 0.0],
			last_orientation: [0.0, 0.0, 0.0],
			cached_azimuth: 0.0,
			is_azimuth_elevation_dirty: true,
			cached_distance_cone_gain: 1.0,
			is_distance_cone_gain_dirty: true,
			last_non_silent_time: 0.0,
			value_list: [[0.0; RENDER_QUANTUM_FRAMES]; 6],
			azimuth_value_list: [0.0; RENDER_QUANTUM_FRAMES],
			total_gain_value_list: [0.0; RENDER_QUANTUM_FRAMES],
		}
	}

	//==============================================================================
	/// 설정 바꾸기. (SetRefDistance 들처럼 값이 바뀐 거리, 원뿔 값만 이득을 dirty 로 건다)
	//==============================================================================
	pub(crate) fn set_settings(&mut self, settings: PannerSettings) {
		let previous = self.settings;
		let is_gain_changed = previous.reference_distance != settings.reference_distance || previous.maximum_distance != settings.maximum_distance || previous.rolloff_factor != settings.rolloff_factor || previous.cone_inner_angle != settings.cone_inner_angle || previous.cone_outer_angle != settings.cone_outer_angle || previous.cone_outer_gain != settings.cone_outer_gain;
		self.settings = settings;
		if is_gain_changed {
			self.is_distance_cone_gain_dirty = true;
		}
	}

	//==============================================================================
	/// 자동화 받기.
	//==============================================================================
	pub(crate) fn apply_automation(&mut self, param: PannerParam, automation: ParamAutomation, current_frame: u64, sample_rate: f64) {
		self.param_list[param as usize].apply_automation(automation, current_frame, sample_rate);
	}

	/// FinalValue() 셋. (a-rate 의 k-rate 읽기)
	fn read_vector(&mut self, first_index: usize, current_frame: u64, sample_rate: f64) -> Vector3F {
		let mut vector = [0.0f32; 3];
		for (offset, value) in vector.iter_mut().enumerate() {
			*value = self.param_list[first_index + offset].calculate_quantum_value(current_frame, sample_rate, RENDER_QUANTUM_FRAMES);
		}
		vector
	}

	//==============================================================================
	/// 이 퀀텀에 표본마다 셈할 자동화가 있는지.
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 하나라도 있으면 true.
	//==============================================================================
	fn has_sample_accurate_values(&self, current_frame: u64, sample_rate: f64) -> bool {
		self.param_list.iter().any(|param| param.has_sample_accurate_values(current_frame, sample_rate, RENDER_QUANTUM_FRAMES))
	}

	//==============================================================================
	/// AudioParam 만 흘린다. (ProcessOnlyAudioParams)
	///
	/// # 인자
	/// - `current_frame`: 지금 퀀텀의 첫 프레임.
	/// - `sample_rate`: 표본율.
	//==============================================================================
	fn process_only_audio_params(&mut self, current_frame: u64, sample_rate: f64) {
		for (param, value_list) in self.param_list.iter_mut().zip(self.value_list.iter_mut()) {
			param.calculate_sample_accurate_values(value_list, current_frame, sample_rate);
		}
	}

	//==============================================================================
	/// 퀀텀 하나. (ProcessIfNecessary, quantum 의 input_channel_count 채널을 받아 두 채널로 바꿔 쓴다. 소리를 냈으면 참)
	//==============================================================================
	pub(crate) fn process(&mut self, quantum: &mut Quantum, input_channel_count: usize, is_input_silent: bool, listener: &mut ListenerState, current_frame: u64, sample_rate: f64) -> bool {
		let current_time = current_frame as f64 / sample_rate;
		let has_sound = if is_input_silent && self.last_non_silent_time < current_time {
			// 침묵을 넘긴다. (꼬리와 지연은 등력이라 0)
			self.process_only_audio_params(current_frame, sample_rate);
			false
		}
		else {
			if is_input_silent {
				quantum[0].fill(0.0);
				quantum[1].fill(0.0);
			}
			self.render(quantum, input_channel_count.clamp(1, 2), listener, current_frame, sample_rate);
			!is_input_silent
		};
		if !is_input_silent {
			self.last_non_silent_time = (current_frame + RENDER_QUANTUM_FRAMES as u64) as f64 / sample_rate;
		}
		has_sound
	}

	//==============================================================================
	/// 좌우와 이득. (Process)
	//==============================================================================
	fn render(&mut self, quantum: &mut Quantum, input_channel_count: usize, listener: &mut ListenerState, current_frame: u64, sample_rate: f64) {
		if self.has_sample_accurate_values(current_frame, sample_rate) || listener.has_sample_accurate_values(current_frame, sample_rate) {
			self.render_sample_accurate(quantum, input_channel_count, listener, current_frame, sample_rate);
			return;
		}
		// UpdateDirtyState.
		let current_position = self.read_vector(0, current_frame, sample_rate);
		let current_orientation = self.read_vector(3, current_frame, sample_rate);
		if current_position != self.last_position || current_orientation != self.last_orientation {
			self.last_position = current_position;
			self.last_orientation = current_orientation;
			self.is_azimuth_elevation_dirty = true;
			self.is_distance_cone_gain_dirty = true;
		}
		if self.is_azimuth_elevation_dirty || listener.is_listener_dirty {
			let position = self.read_vector(0, current_frame, sample_rate);
			let listener_position = listener.get_position(current_frame, sample_rate);
			let listener_forward = listener.get_orientation(current_frame, sample_rate);
			let listener_up = listener.get_up_vector(current_frame, sample_rate);
			let (azimuth, _elevation) = calculate_azimuth_elevation_f(position, listener_position, listener_forward, listener_up);
			self.cached_azimuth = azimuth;
			self.is_azimuth_elevation_dirty = false;
		}
		pan_quantum(quantum, input_channel_count, self.cached_azimuth);
		// DistanceConeGain.
		if self.is_distance_cone_gain_dirty || listener.is_listener_dirty {
			let position = self.read_vector(0, current_frame, sample_rate);
			let orientation = self.read_vector(3, current_frame, sample_rate);
			let listener_position = listener.get_position(current_frame, sample_rate);
			self.cached_distance_cone_gain = calculate_distance_cone_gain_f(&self.settings, position, orientation, listener_position);
			self.is_distance_cone_gain_dirty = false;
		}
		// CopyWithGainFrom. (1 이면 그대로, 0 이면 0)
		let total_gain = self.cached_distance_cone_gain;
		if total_gain == 1.0 {
			return;
		}
		for channel in quantum.iter_mut() {
			for sample in channel.iter_mut() {
				*sample = if total_gain == 0.0 { 0.0 } else { *sample * total_gain };
			}
		}
	}

	//==============================================================================
	/// 표본마다. (ProcessSampleAccurateValues)
	//==============================================================================
	fn render_sample_accurate(&mut self, quantum: &mut Quantum, input_channel_count: usize, listener: &mut ListenerState, current_frame: u64, sample_rate: f64) {
		self.process_only_audio_params(current_frame, sample_rate);
		listener.update_values_if_needed(current_frame, sample_rate);
		for frame_index in 0..RENDER_QUANTUM_FRAMES {
			let position = [self.value_list[0][frame_index], self.value_list[1][frame_index], self.value_list[2][frame_index]];
			let orientation = [self.value_list[3][frame_index], self.value_list[4][frame_index], self.value_list[5][frame_index]];
			let listener_values = &listener.value_list;
			let listener_position = [listener_values[0][frame_index], listener_values[1][frame_index], listener_values[2][frame_index]];
			let listener_forward = [listener_values[3][frame_index], listener_values[4][frame_index], listener_values[5][frame_index]];
			let listener_up = [listener_values[6][frame_index], listener_values[7][frame_index], listener_values[8][frame_index]];
			let (azimuth, _elevation) = calculate_azimuth_elevation_f(position, listener_position, listener_forward, listener_up);
			self.azimuth_value_list[frame_index] = azimuth;
			self.total_gain_value_list[frame_index] = calculate_distance_cone_gain_f(&self.settings, position, orientation, listener_position);
		}
		self.cached_azimuth = self.azimuth_value_list[RENDER_QUANTUM_FRAMES - 1];
		self.cached_distance_cone_gain = self.total_gain_value_list[RENDER_QUANTUM_FRAMES - 1];
		let [left, right] = quantum;
		for frame_index in 0..RENDER_QUANTUM_FRAMES {
			let (output_left, output_right) = pan_sample(self.azimuth_value_list[frame_index], input_channel_count, left[frame_index], right[frame_index]);
			let total_gain = self.total_gain_value_list[frame_index];
			left[frame_index] = output_left * total_gain;
			right[frame_index] = output_right * total_gain;
		}
	}
}

//==============================================================================
/// 등력 좌우를 표본 하나에. (EqualPowerPanner, 모노는 왼쪽 칸만 읽는다)
//==============================================================================
fn pan_sample(azimuth: f64, input_channel_count: usize, input_left: f32, input_right: f32) -> (f32, f32) {
	super::pannermath::pan_equal_power_sample(azimuth, input_channel_count, input_left, input_right)
}

//==============================================================================
/// 등력 좌우를 퀀텀에. (EqualPowerPanner::Pan, 이득은 퀀텀에 한 번)
//==============================================================================
fn pan_quantum(quantum: &mut Quantum, input_channel_count: usize, azimuth: f64) {
	let (gain_left, gain_right) = calculate_equal_power_gains(azimuth, input_channel_count);
	let is_left_side = alias_azimuth(azimuth) <= 0.0;
	let [left, right] = quantum;
	for frame_index in 0..RENDER_QUANTUM_FRAMES {
		let input_left = left[frame_index];
		let input_right = if input_channel_count == 1 { input_left } else { right[frame_index] };
		if input_channel_count == 1 {
			left[frame_index] = (input_left as f64 * gain_left) as f32;
			right[frame_index] = (input_left as f64 * gain_right) as f32;
		}
		else if is_left_side {
			left[frame_index] = (input_left as f64 + input_right as f64 * gain_left) as f32;
			right[frame_index] = (input_right as f64 * gain_right) as f32;
		}
		else {
			left[frame_index] = (input_left as f64 * gain_left) as f32;
			right[frame_index] = (input_right as f64 + input_left as f64 * gain_right) as f32;
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::mixer::{AudioBufferData, MixerCommand, MixerRenderer, MixerShared};
	use super::super::pannermath::{DistanceModel, PanningModel};
	use super::*;
	use std::sync::atomic::{AtomicU32, AtomicU64};
	use std::sync::mpsc::channel;
	use std::sync::Arc;

	//==============================================================================
	/// JSON 의 PannerNode AudioParam 이름.
	///
	/// # 인자
	/// - `name`: 이름.
	///
	/// # 돌려줌
	/// PannerParam.
	//==============================================================================
	fn read_panner_param(name: &str) -> PannerParam {
		match name {
			"positionX" => PannerParam::PositionX,
			"positionY" => PannerParam::PositionY,
			"positionZ" => PannerParam::PositionZ,
			"orientationX" => PannerParam::OrientationX,
			"orientationY" => PannerParam::OrientationY,
			_ => PannerParam::OrientationZ,
		}
	}

	//==============================================================================
	/// JSON 의 리스너 AudioParam 이름.
	///
	/// # 인자
	/// - `name`: 이름.
	///
	/// # 돌려줌
	/// ListenerParam.
	//==============================================================================
	fn read_listener_param(name: &str) -> ListenerParam {
		match name {
			"positionX" => ListenerParam::PositionX,
			"positionY" => ListenerParam::PositionY,
			"positionZ" => ListenerParam::PositionZ,
			"forwardX" => ListenerParam::ForwardX,
			"forwardY" => ListenerParam::ForwardY,
			"forwardZ" => ListenerParam::ForwardZ,
			"upX" => ListenerParam::UpX,
			"upY" => ListenerParam::UpY,
			_ => ListenerParam::UpZ,
		}
	}

	//==============================================================================
	/// JSON 의 자동화 하나. (프레임으로 적힌 시각은 프레임 / 표본율, serde_json 의 실수 읽기가 끝자리를 어긋낼 수 있어 경계 시각은 프레임으로 적는다)
	///
	/// # 인자
	/// - `automation`: 자동화 객체.
	/// - `context_rate`: 컨텍스트 표본율.
	///
	/// # 돌려줌
	/// 믹서 AudioParam 자동화.
	//==============================================================================
	fn read_automation(automation: &serde_json::Value, context_rate: f64) -> ParamAutomation {
		let value = automation["value"].as_f64().unwrap_or(0.0) as f32;
		let time = match automation["frame"].as_f64() {
			Some(frame) => frame / context_rate,
			None => automation["time"].as_f64().unwrap(),
		};
		match automation["type"].as_str().unwrap() {
			"setValueAtTime" => ParamAutomation::SetValueAtTime { value, time },
			"linearRamp" => ParamAutomation::LinearRampToValueAtTime { value, time },
			"exponentialRamp" => ParamAutomation::ExponentialRampToValueAtTime { value, time },
			_ => ParamAutomation::SetTargetAtTime { target: value, time, time_constant: automation["timeConstant"].as_f64().unwrap() },
		}
	}

	//==============================================================================
	/// JSON 의 PannerNode 속성을 설정에 넣는다.
	///
	/// # 인자
	/// - `settings`: 고칠 설정.
	/// - `json`: 속성 객체.
	//==============================================================================
	fn apply_settings_json(settings: &mut PannerSettings, json: &serde_json::Value) {
		for (name, value) in json.as_object().unwrap() {
			match name.as_str() {
				"distanceModel" => {
					settings.distance_model = match value.as_str().unwrap() {
						"linear" => DistanceModel::Linear,
						"exponential" => DistanceModel::Exponential,
						_ => DistanceModel::Inverse,
					};
				}
				"refDistance" => settings.reference_distance = value.as_f64().unwrap(),
				"maxDistance" => settings.maximum_distance = value.as_f64().unwrap(),
				"rolloffFactor" => settings.rolloff_factor = value.as_f64().unwrap(),
				"coneInnerAngle" => settings.cone_inner_angle = value.as_f64().unwrap(),
				"coneOuterAngle" => settings.cone_outer_angle = value.as_f64().unwrap(),
				"coneOuterGain" => settings.cone_outer_gain = value.as_f64().unwrap(),
				_ => {}
			}
		}
	}

	//==============================================================================
	/// 크롬(158) OfflineAudioContext 의 PannerNode 그래프와 견준다. (tests/assets/spatialaudio/chromespatial.cjs 가 찍은 chrome_spatial.json)
	/// - 등력 좌우(모노, 스테레오 양쪽), 거리 모델 셋과 모서리(같은 자리, 기준과 최대가 같음), 원뿔, 돈 리스너, 버스.
	/// - 자리와 리스너 자동화(표본마다 셈), 멈춰 값 대입과 설정 바꾸기(크롬처럼 거리 모델만 바꾸면 다시 셈하지 않음), GainNode 0 동안의 침묵 넘기기.
	//==============================================================================
	#[test]
	fn panner_matches_chrome_offline_rendering() {
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/spatialaudio/chrome_spatial.json")).expect("chrome_spatial.json")).expect("json");
		let buffer_sample_rate = json["bufferSampleRate"].as_f64().unwrap();
		let buffer_length = json["bufferLength"].as_u64().unwrap() as usize;
		let render_length = json["renderLength"].as_u64().unwrap() as usize;
		let index_list: Vec<usize> = json["index"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as usize).collect();
		let create_channel = |multiplier: usize, modulus: usize, offset: f64, divisor: f64| -> Vec<f32> {
			(0..buffer_length).map(|index| ((((index * multiplier) % modulus) as f64 - offset) / divisor) as f32).collect()
		};
		let stereo_buffer = Arc::new(AudioBufferData { sample_rate: buffer_sample_rate, channel_list: vec![create_channel(37, 200, 100.0, 250.0), create_channel(53, 160, 80.0, 200.0)] });
		let mono_buffer = Arc::new(AudioBufferData { sample_rate: buffer_sample_rate, channel_list: vec![create_channel(29, 180, 90.0, 225.0)] });
		let case_list = json["cases"].as_array().unwrap();
		assert_eq!(case_list.len(), 9);
		let mut maximum_difference = 0.0f64;
		for test_case in case_list {
			let name = test_case["name"].as_str().unwrap();
			let context_rate = test_case["contextRate"].as_f64().unwrap();
			let (command_sender, command_receiver) = channel();
			let mut renderer = MixerRenderer::new(command_receiver, Arc::new(MixerShared::new(context_rate)), context_rate);
			// 리스너: 값 아홉을 대입하고 자동화.
			let listener = &test_case["listener"];
			let listener_value_list: Vec<f64> = ["position", "forward", "up"].iter().flat_map(|key| listener[*key].as_array().unwrap().iter().map(|value| value.as_f64().unwrap())).collect();
			for (param_index, value) in listener_value_list.iter().enumerate() {
				let param = read_listener_param(["positionX", "positionY", "positionZ", "forwardX", "forwardY", "forwardZ", "upX", "upY", "upZ"][param_index]);
				command_sender.send(MixerCommand::ListenerAutomation { param, automation: ParamAutomation::SetValue { value: *value as f32 } }).unwrap();
			}
			for automation in listener["automation"].as_array().unwrap() {
				command_sender.send(MixerCommand::ListenerAutomation { param: read_listener_param(automation["param"].as_str().unwrap()), automation: read_automation(automation, context_rate) }).unwrap();
			}
			// 플레이어: (버스) GainNode → PannerNode.
			let player = &test_case["player"];
			let output_bus_id = player["busGain"].as_f64().map(|bus_gain| {
				command_sender.send(MixerCommand::AddBus { bus_id: 1 }).unwrap();
				command_sender.send(MixerCommand::BusGainAutomation { bus_id: 1, automation: ParamAutomation::SetValue { value: bus_gain as f32 } }).unwrap();
				1
			});
			let player_id = 2;
			command_sender.send(MixerCommand::AddPlayer { player_id, output_bus_id, ended_generation: Arc::new(AtomicU64::new(0)), gain_value: Arc::new(AtomicU32::new(0)) }).unwrap();
			let panner = &test_case["panner"];
			let mut settings = PannerSettings { panning_model: PanningModel::EqualPower, ..PannerSettings::default() };
			apply_settings_json(&mut settings, &serde_json::json!({
				"distanceModel": panner["distanceModel"], "refDistance": panner["refDistance"], "maxDistance": panner["maxDistance"], "rolloffFactor": panner["rolloffFactor"],
				"coneInnerAngle": panner["coneInnerAngle"], "coneOuterAngle": panner["coneOuterAngle"], "coneOuterGain": panner["coneOuterGain"],
			}));
			command_sender.send(MixerCommand::SetPlayerPanner { player_id, settings }).unwrap();
			let panner_value_list: Vec<f64> = ["position", "orientation"].iter().flat_map(|key| panner[*key].as_array().unwrap().iter().map(|value| value.as_f64().unwrap())).collect();
			for (param_index, value) in panner_value_list.iter().enumerate() {
				let param = read_panner_param(["positionX", "positionY", "positionZ", "orientationX", "orientationY", "orientationZ"][param_index]);
				command_sender.send(MixerCommand::PlayerPannerAutomation { player_id, param, automation: ParamAutomation::SetValue { value: *value as f32 } }).unwrap();
			}
			for automation in panner["automation"].as_array().unwrap() {
				command_sender.send(MixerCommand::PlayerPannerAutomation { player_id, param: read_panner_param(automation["param"].as_str().unwrap()), automation: read_automation(automation, context_rate) }).unwrap();
			}
			command_sender.send(MixerCommand::PlayerGainAutomation { player_id, automation: ParamAutomation::SetValue { value: player["gain"].as_f64().unwrap() as f32 } }).unwrap();
			for automation in player["gainAutomation"].as_array().unwrap() {
				command_sender.send(MixerCommand::PlayerGainAutomation { player_id, automation: read_automation(automation, context_rate) }).unwrap();
			}
			let audio_buffer = if player["channels"].as_str().unwrap() == "stereo" { stereo_buffer.clone() } else { mono_buffer.clone() };
			command_sender.send(MixerCommand::StartSource { player_id, audio_buffer, is_loop: player["loop"].as_bool().unwrap(), offset_seconds: player["offset"].as_f64().unwrap(), playback_rate: player["rate"].as_f64().unwrap(), generation: 1 }).unwrap();
			if let Some(stop_time) = player["stop"].as_f64() {
				command_sender.send(MixerCommand::StopSource { player_id, time: stop_time }).unwrap();
			}
			// 멈춤마다 값 대입과 설정.
			let mut output = vec![0.0f32; render_length * 2];
			let mut rendered_frame_count = 0usize;
			for suspend in test_case["suspendList"].as_array().unwrap() {
				let suspend_frame = suspend["frame"].as_u64().unwrap() as usize;
				renderer.render_interleaved(&mut output[rendered_frame_count * 2..suspend_frame * 2], 2);
				rendered_frame_count = suspend_frame;
				for action in suspend["actions"].as_array().unwrap() {
					match action["kind"].as_str().unwrap() {
						"pannerValue" => {
							command_sender.send(MixerCommand::PlayerPannerAutomation { player_id, param: read_panner_param(action["param"].as_str().unwrap()), automation: ParamAutomation::SetValue { value: action["value"].as_f64().unwrap() as f32 } }).unwrap();
						}
						"listenerValue" => {
							command_sender.send(MixerCommand::ListenerAutomation { param: read_listener_param(action["param"].as_str().unwrap()), automation: ParamAutomation::SetValue { value: action["value"].as_f64().unwrap() as f32 } }).unwrap();
						}
						_ => {
							apply_settings_json(&mut settings, &action["settings"]);
							command_sender.send(MixerCommand::SetPlayerPanner { player_id, settings }).unwrap();
						}
					}
				}
			}
			renderer.render_interleaved(&mut output[rendered_frame_count * 2..], 2);
			for channel_index in 0..2 {
				let value_list = test_case["values"][channel_index].as_array().unwrap();
				for (index, value) in index_list.iter().zip(value_list.iter()) {
					let expected = value.as_f64().unwrap();
					let actual = output[index * 2 + channel_index] as f64;
					maximum_difference = maximum_difference.max((actual - expected).abs());
					assert!((actual - expected).abs() < 1e-6, "{} 채널 {} 자리 {}: {} / {}", name, channel_index, index, actual, expected);
				}
				let energy: f64 = (0..render_length).map(|index| (output[index * 2 + channel_index] as f64).powi(2)).sum();
				let expected_energy = test_case["energy"][channel_index].as_f64().unwrap();
				assert!((energy - expected_energy).abs() <= expected_energy * 1e-6 + 1e-9, "{} 채널 {} 제곱합: {} / {}", name, channel_index, energy, expected_energy);
			}
		}
		println!("PannerNode 크롬과 가장 큰 차이: {:e}", maximum_difference);
	}
}
