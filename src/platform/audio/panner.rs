//==============================================================================
//! 데스크탑 믹서의 PannerNode 와 AudioListener. (크로미엄 panner_handler.cc, audio_listener_handler.cc 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다. (experimental::spatialaudio 의 SpatialAudioPlayer, SpatialAudioListener 가 쓴다)
//! - 자리와 방향은 a-rate AudioParam 이다. (audioparam.rs) 퀀텀마다 PannerHandler::ProcessIfNecessary 와 같은 차례로 셈한다.
//!   입력이 조용하고 침묵을 넘길 때(마지막 소리 + 늦음 + 꼬리 뒤 퀀텀부터, PropagatesSilence)는 AudioParam 만 흘리고(ProcessOnlyAudioParams),
//!   자리나 리스너에 이 퀀텀 자동화가 있으면 표본마다(ProcessSampleAccurateValues), 아니면 바뀐 때만 방위각과 이득을 다시 셈한다.
//! - 리스너는 렌더 퀀텀마다 그리기 전에 UpdateState(값을 읽어 바뀌었는지)를 한다. (OfflineAudioContext::HandlePreRenderTasks)
//! - 좌우는 등력(equal_power_panner.cc)과 HRTF(hrtf/, hrtf_panner.cc)다. 좌우 방식이 바뀌면 크롬 SetPanningModel 처럼 좌우를 새로 짓는다.
//!   HRTF 데이터베이스 로더는 리스너가 표본율마다 든다. (AudioListenerHandler::CreateAndLoadHRTFDatabaseLoader) 등력은 꼬리와 늦음이 0 이고,
//!   HRTF 는 꼬리(지연 최대 2 ms + FFT 크기 / 2)와 늦음(FFT 크기 / 2) 동안 조용한 입력으로도 소리를 낸다.
//! - 크로미엄처럼 거리 모델만 바꾸면 이득을 다시 셈하지 않는다. (SetDistanceModel 은 dirty 를 걸지 않는다) 기준, 최대, rolloff, 원뿔 값은 바뀌면 건다.
//==============================================================================
use super::audioparam::{AudioParam, ParamAutomation};
use super::hrtf::{HrtfDatabaseLoader, HrtfPanner};
use super::mixer::RENDER_QUANTUM_FRAMES;
use super::pannermath::{alias_azimuth, calculate_azimuth_elevation_f, calculate_distance_cone_gain_f, calculate_equal_power_gains, PannerSettings, PanningModel, Vector3F};
use super::{ListenerParam, PannerParam};
use std::sync::Arc;

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
	/// HRTF 데이터베이스 로더. (처음 HRTF 를 고를 때 그 표본율로 짓고, 표본율이 바뀌면 새로)
	hrtf_database_loader: Option<Arc<HrtfDatabaseLoader>>,
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
			hrtf_database_loader: None,
		}
	}

	//==============================================================================
	/// 표본율의 HRTF 데이터베이스 로더. (CreateAndLoadHRTFDatabaseLoader, 이미 들고 있으면 그것)
	///
	/// # 인자
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 로더.
	//==============================================================================
	fn create_and_load_hrtf_database_loader(&mut self, sample_rate: f32) -> Arc<HrtfDatabaseLoader> {
		if let Some(loader) = self.hrtf_database_loader.as_ref().filter(|loader| loader.get_sample_rate() == sample_rate) {
			return loader.clone();
		}
		let loader = HrtfDatabaseLoader::create_and_load_asynchronously_if_necessary(sample_rate);
		self.hrtf_database_loader = Some(loader.clone());
		loader
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
	/// HRTF 좌우. (좌우 방식이 HRTF 일 때, 그 퀀텀의 표본율로 짓는다. 없으면 등력)
	hrtf_panner: Option<Box<HrtfPanner>>,
	last_position: Vector3F,
	last_orientation: Vector3F,
	cached_azimuth: f64,
	cached_elevation: f64,
	is_azimuth_elevation_dirty: bool,
	cached_distance_cone_gain: f32,
	is_distance_cone_gain_dirty: bool,
	/// 마지막으로 소리가 난 퀀텀의 끝 시각. (초, PropagatesSilence)
	last_non_silent_time: f64,
	value_list: [[f32; RENDER_QUANTUM_FRAMES]; 6],
	azimuth_value_list: [f64; RENDER_QUANTUM_FRAMES],
	elevation_value_list: [f64; RENDER_QUANTUM_FRAMES],
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
			hrtf_panner: None,
			last_position: [0.0, 0.0, 0.0],
			last_orientation: [0.0, 0.0, 0.0],
			cached_azimuth: 0.0,
			cached_elevation: 0.0,
			is_azimuth_elevation_dirty: true,
			cached_distance_cone_gain: 1.0,
			is_distance_cone_gain_dirty: true,
			last_non_silent_time: 0.0,
			value_list: [[0.0; RENDER_QUANTUM_FRAMES]; 6],
			azimuth_value_list: [0.0; RENDER_QUANTUM_FRAMES],
			elevation_value_list: [0.0; RENDER_QUANTUM_FRAMES],
			total_gain_value_list: [0.0; RENDER_QUANTUM_FRAMES],
		}
	}

	//==============================================================================
	/// 설정 바꾸기. (SetRefDistance 들처럼 값이 바뀐 거리, 원뿔 값만 이득을 dirty 로 건다. 좌우 방식이 바뀌면 SetPanningModel 처럼 좌우를 새로 짓는다)
	//==============================================================================
	pub(crate) fn set_settings(&mut self, settings: PannerSettings) {
		let previous = self.settings;
		let is_gain_changed = previous.reference_distance != settings.reference_distance || previous.maximum_distance != settings.maximum_distance || previous.rolloff_factor != settings.rolloff_factor || previous.cone_inner_angle != settings.cone_inner_angle || previous.cone_outer_angle != settings.cone_outer_angle || previous.cone_outer_gain != settings.cone_outer_gain;
		self.settings = settings;
		if is_gain_changed {
			self.is_distance_cone_gain_dirty = true;
		}
		if previous.panning_model != settings.panning_model {
			self.hrtf_panner = None;
		}
	}

	//==============================================================================
	/// HRTF 좌우를 준비한다. (좌우 방식이 HRTF 인데 좌우가 없거나 표본율이 다르면 리스너의 로더로 새로 짓는다)
	///
	/// # 인자
	/// - `listener`: 리스너.
	/// - `sample_rate`: 표본율.
	//==============================================================================
	fn prepare_hrtf_panner(&mut self, listener: &mut ListenerState, sample_rate: f64) {
		if self.settings.panning_model != PanningModel::Hrtf {
			return;
		}
		let sample_rate = sample_rate as f32;
		if self.hrtf_panner.as_ref().map(|hrtf_panner| hrtf_panner.get_sample_rate() == sample_rate).unwrap_or(false) {
			return;
		}
		let database_loader = listener.create_and_load_hrtf_database_loader(sample_rate);
		self.hrtf_panner = Some(Box::new(HrtfPanner::new(sample_rate, database_loader)));
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
	/// 퀀텀 하나. (ProcessIfNecessary, quantum 의 input_channel_count 채널을 받아 두 채널로 바꿔 쓴다. 셈해 출력을 냈으면 참, 침묵을 넘겼으면 거짓)
	//==============================================================================
	pub(crate) fn process(&mut self, quantum: &mut Quantum, input_channel_count: usize, is_input_silent: bool, listener: &mut ListenerState, current_frame: u64, sample_rate: f64) -> bool {
		self.prepare_hrtf_panner(listener, sample_rate);
		let current_time = current_frame as f64 / sample_rate;
		// 늦음과 꼬리. (등력은 0)
		let (latency_time, tail_time) = match self.hrtf_panner.as_ref() {
			Some(hrtf_panner) => {
				(hrtf_panner.get_latency_time(), hrtf_panner.get_tail_time())
			}
			None => {
				(0.0, 0.0)
			}
		};
		let has_sound = if is_input_silent && self.last_non_silent_time + latency_time + tail_time < current_time {
			// 침묵을 넘긴다.
			self.process_only_audio_params(current_frame, sample_rate);
			false
		}
		else {
			if is_input_silent {
				quantum[0].fill(0.0);
				quantum[1].fill(0.0);
			}
			self.render(quantum, input_channel_count.clamp(1, 2), listener, current_frame, sample_rate);
			true
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
			let (azimuth, elevation) = calculate_azimuth_elevation_f(position, listener_position, listener_forward, listener_up);
			self.cached_azimuth = azimuth;
			self.cached_elevation = elevation;
			self.is_azimuth_elevation_dirty = false;
		}
		match self.hrtf_panner.as_mut() {
			Some(hrtf_panner) => {
				hrtf_panner.pan(self.cached_azimuth, self.cached_elevation, quantum, input_channel_count);
			}
			None => {
				pan_quantum(quantum, input_channel_count, self.cached_azimuth);
			}
		}
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
	/// 표본마다. (ProcessSampleAccurateValues, HRTF 는 크롬처럼 첫 표본의 방위와 높이로 셈하고 이득만 표본마다 곱한다)
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
			let (azimuth, elevation) = calculate_azimuth_elevation_f(position, listener_position, listener_forward, listener_up);
			self.azimuth_value_list[frame_index] = azimuth;
			self.elevation_value_list[frame_index] = elevation;
			self.total_gain_value_list[frame_index] = calculate_distance_cone_gain_f(&self.settings, position, orientation, listener_position);
		}
		self.cached_azimuth = self.azimuth_value_list[RENDER_QUANTUM_FRAMES - 1];
		self.cached_elevation = self.elevation_value_list[RENDER_QUANTUM_FRAMES - 1];
		self.cached_distance_cone_gain = self.total_gain_value_list[RENDER_QUANTUM_FRAMES - 1];
		if let Some(hrtf_panner) = self.hrtf_panner.as_mut() {
			// PanWithSampleAccurateValues 는 첫 표본의 값으로 Pan 한다. (CopyWithSampleAccurateGainValuesFrom)
			hrtf_panner.pan(self.azimuth_value_list[0], self.elevation_value_list[0], quantum, input_channel_count);
			for channel in quantum.iter_mut() {
				for (sample, total_gain) in channel.iter_mut().zip(self.total_gain_value_list.iter()) {
					*sample *= *total_gain;
				}
			}
			return;
		}
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
	use super::super::pannermath::DistanceModel;
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
				"panningModel" => {
					settings.panning_model = if value.as_str().unwrap() == "HRTF" { PanningModel::Hrtf } else { PanningModel::EqualPower };
				}
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
	/// 크롬(158) OfflineAudioContext 의 PannerNode 그래프를 그려 견준다. (tests/assets/spatialaudio/chromespatial.cjs 가 찍은 JSON)
	/// - HRTF 가 낀 그래프는 크롬 OfflineAudioContext 처럼 그 표본율의 데이터베이스를 다 지은 뒤 그린다. (WaitForHRTFDatabaseLoaderThreadCompletion)
	///
	/// # 인자
	/// - `file_name`: tests/assets/spatialaudio 안의 JSON 이름.
	/// - `case_count`: 그래프 수.
	/// - `tolerance`: 표본 하나의 차이 한계.
	///
	/// # 돌려줌
	/// (경우 이름, 가장 큰 차이, 제곱합의 상대 차이) 목록.
	//==============================================================================
	fn compare_with_chrome_rendering(file_name: &str, case_count: usize, tolerance: f64) -> Vec<(String, f64, f64)> {
		let json_path = format!("{}/tests/assets/spatialaudio/{}", env!("CARGO_MANIFEST_DIR"), file_name);
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&json_path).expect(file_name)).expect("json");
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
		assert_eq!(case_list.len(), case_count);
		let mut result_list = Vec::new();
		for test_case in case_list {
			let name = test_case["name"].as_str().unwrap();
			let context_rate = test_case["contextRate"].as_f64().unwrap();
			let hrtf_database_loader = serde_json::to_string(test_case).unwrap().contains("\"HRTF\"").then(|| HrtfDatabaseLoader::create_and_load_asynchronously_if_necessary(context_rate as f32));
			if let Some(loader) = hrtf_database_loader.as_ref() {
				loader.wait_for_loader_thread_completion();
			}
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
			let mut settings = PannerSettings::default();
			apply_settings_json(&mut settings, &serde_json::json!({
				"panningModel": panner["panningModel"], "distanceModel": panner["distanceModel"], "refDistance": panner["refDistance"], "maxDistance": panner["maxDistance"], "rolloffFactor": panner["rolloffFactor"],
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
			let mut maximum_difference = 0.0f64;
			let mut maximum_energy_ratio = 0.0f64;
			for channel_index in 0..2 {
				let value_list = test_case["values"][channel_index].as_array().unwrap();
				for (index, value) in index_list.iter().zip(value_list.iter()) {
					let expected = value.as_f64().unwrap();
					let actual = output[index * 2 + channel_index] as f64;
					maximum_difference = maximum_difference.max((actual - expected).abs());
					assert!((actual - expected).abs() < tolerance, "{} 채널 {} 자리 {}: {} / {}", name, channel_index, index, actual, expected);
				}
				let energy: f64 = (0..render_length).map(|index| (output[index * 2 + channel_index] as f64).powi(2)).sum();
				let expected_energy = test_case["energy"][channel_index].as_f64().unwrap();
				maximum_energy_ratio = maximum_energy_ratio.max((energy - expected_energy).abs() / expected_energy.max(1e-12));
				assert!((energy - expected_energy).abs() <= expected_energy * 1e-6 + 1e-9, "{} 채널 {} 제곱합: {} / {}", name, channel_index, energy, expected_energy);
			}
			result_list.push((String::from(name), maximum_difference, maximum_energy_ratio));
		}
		result_list
	}

	//==============================================================================
	/// 크롬(158) OfflineAudioContext 의 등력 PannerNode 그래프와 견준다. (chrome_spatial.json)
	/// - 등력 좌우(모노, 스테레오 양쪽), 거리 모델 셋과 모서리(같은 자리, 기준과 최대가 같음), 원뿔, 돈 리스너, 버스.
	/// - 자리와 리스너 자동화(표본마다 셈), 멈춰 값 대입과 설정 바꾸기(크롬처럼 거리 모델만 바꾸면 다시 셈하지 않음), GainNode 0 동안의 침묵 넘기기.
	//==============================================================================
	#[test]
	fn panner_matches_chrome_offline_rendering() {
		let result_list = compare_with_chrome_rendering("chrome_spatial.json", 10, 1e-6);
		let maximum_difference = result_list.iter().map(|(_, difference, _)| *difference).fold(0.0f64, f64::max);
		println!("PannerNode 크롬과 가장 큰 차이: {:e}", maximum_difference);
	}

	//==============================================================================
	/// 크롬(158) OfflineAudioContext 의 HRTF PannerNode 그래프와 견준다. (chrome_spatial_hrtf.json)
	/// - 표본율 22050, 32000, 44100, 48000, 96000, 모노와 스테레오, 위아래 끝 높이, 움직이는 소리와 돈 리스너의 크로스페이드,
	///   그리는 도중 좌우 방식 바꾸기, 조용한 입력과 소스가 끝난 뒤의 꼬리.
	//==============================================================================
	#[test]
	fn hrtf_panner_matches_chrome_offline_rendering() {
		let result_list = compare_with_chrome_rendering("chrome_spatial_hrtf.json", 9, 1e-6);
		for (name, difference, energy_ratio) in result_list.iter() {
			println!("HRTF {}: 가장 큰 차이 {:e}, 제곱합 상대 차이 {:e}", name, difference, energy_ratio);
		}
	}
}
