//==============================================================================
//! 데스크탑 믹서. (cpal 의 소리 스레드에서 도는 WebAudio 노드 그래프, 원본이 쓰는 크롬 WebAudio 와 같은 셈)
//! - 그래프: AudioBufferSourceNode → GainNode(플레이어) → `[StereoPannerNode]` → GainNode(버스) 또는 출력. 비프는 OscillatorNode → GainNode → 출력.
//!   원본 AudioPlayer, AudioBus, SoundSystem.playEffect, BeepPlayer 가 짓는 그래프와 같다. (웹 백엔드 web.rs 와도 같다)
//! - 128 프레임 렌더 퀀텀마다 그린다. (크롬 kRenderQuantumFrames) 명령은 퀀텀을 그리기 전에 받아, 값 바꾸기와 시작, 정지가 퀀텀 경계에서 걸린다.
//! - 소스: 크롬 audio_buffer_source_handler.cc 를 옮겼다. 시작 위치(TimeToSampleFrame 의 1024 배 오버샘플 반올림), 재생 빠르기(버퍼 표본율 / 컨텍스트 표본율 x playbackRate),
//!   빠르기 1 이고 위치가 정수면 그대로 베끼고(ProcessFastPath), 아니면 선형 보간(ProcessInterpolatedPath, 끝에서는 선형 외삽), 루프는 버퍼 전체를 되풀이한다.
//! - GainNode: gain 은 AudioParam 이다. (audioparam.rs, 크로미엄과 같은 자동화 셈) 자동화가 있는 퀀텀은 표본마다, 없으면 퀀텀마다 하나로 곱하고,
//!   0 이면 소리를 낸다고 치지 않는다. 소스가 없는 동안에도 자동화는 흐른다. (GainHandler::Process, ProcessOnlyAudioParams)
//! - 소스의 stop(when): 그 시각을 올림한 프레임에서 끊고 끝낸다. (AudioScheduledSourceHandler::UpdateSchedulingInfo)
//! - OscillatorNode: 크롬 oscillator_handler.cc 와 CPU 별 커널을 옮겼다. (oscillator.rs) 대역 제한 표, 시작과 정지 프레임, frequency 자동화, 표본마다 같은 차례의 읽기와 보간.
//! - StereoPannerNode: 크롬 stereo_panner.cc 의 PanToTargetValue. 모노는 x = (pan + 1) / 2 로 L = in cos(x π/2), R = in sin(x π/2),
//!   스테레오는 pan <= 0 이면 L = L + R cos((pan + 1) π/2), R = R sin((pan + 1) π/2), 아니면 L = L cos(pan π/2), R = R + L sin(pan π/2). 이득은 f64 로 셈해 f32 로 곱한다.
//! - 섞기: 모노를 스테레오 입력에 섞을 때는 스피커 규칙으로 양쪽에 같게 올린다. 출력에 내보낼 때 크롬처럼 -1 ~ 1 로 자른다.
//! - 컨텍스트 멈춤(suspend) 동안은 그리지 않고 시계도 서며, 장치에는 0 을 낸다.
//! - PannerNode(초안): 플레이어에 공간 소리를 걸면 GainNode(와 StereoPannerNode) 뒤에 크롬 PannerHandler 를 옮긴 노드를 끼운다. (panner.rs)
//!   리스너는 컨텍스트에 하나이고, 퀀텀마다 그리기 전에 상태를 읽는다. 입력이 조용해도 PannerNode 가 침묵을 넘기기 전(HRTF 는 꼬리와 늦음 동안)에는
//!   셈한 출력을 내보낼 곳에 섞는다.
//==============================================================================
use super::audioparam::{AudioParam, ParamAutomation};
use super::oscillator::{OscillatorNode, PeriodicWave};
use super::panner::{ListenerState, PannerNode};
use super::pannermath::PannerSettings;
use super::{ListenerParam, PannerParam};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

/// 렌더 퀀텀 프레임 수. (크롬 kRenderQuantumFrames)
pub(crate) const RENDER_QUANTUM_FRAMES: usize = 128;
/// 재생 빠르기 한계. (크롬 kMaxRate)
const MAXIMUM_PLAYBACK_RATE: f64 = 1024.0;

//==============================================================================
/// 푼 소리. (원본 AudioBuffer 자리, 1 채널이나 2 채널, decode 가 크롬처럼 푼 것)
//==============================================================================
pub struct AudioBufferData {
	pub(crate) sample_rate: f64,
	pub(crate) channel_list: Vec<Vec<f32>>,
}

impl AudioBufferData {
	//==============================================================================
	/// 프레임 수. (첫 채널의 길이, 채널이 없으면 0)
	//==============================================================================
	pub(crate) fn get_length(&self) -> usize {
		self.channel_list.first().map(|channel| channel.len()).unwrap_or(0)
	}
}

//==============================================================================
/// 소리 스레드에 보내는 명령.
//==============================================================================
pub(crate) enum MixerCommand {
	AddBus {
		bus_id: u64,
	},
	RemoveBus {
		bus_id: u64,
	},
	BusGainAutomation {
		bus_id: u64,
		automation: ParamAutomation,
	},
	AddPlayer {
		player_id: u64,
		output_bus_id: Option<u64>,
		ended_generation: Arc<AtomicU64>,
		gain_value: Arc<AtomicU32>,
	},
	RemovePlayer {
		player_id: u64,
	},
	PlayerGainAutomation {
		player_id: u64,
		automation: ParamAutomation,
	},
	SetPlayerPan {
		player_id: u64,
		pan: f32,
	},
	/// 공간 소리 설정. (처음이면 PannerNode 를 끼운다, 초안)
	SetPlayerPanner {
		player_id: u64,
		settings: PannerSettings,
	},
	/// PannerNode 의 자리와 방향 자동화. (초안)
	PlayerPannerAutomation {
		player_id: u64,
		param: PannerParam,
		automation: ParamAutomation,
	},
	/// 리스너의 자리, 앞, 위 자동화. (초안)
	ListenerAutomation {
		param: ListenerParam,
		automation: ParamAutomation,
	},
	StartSource {
		player_id: u64,
		audio_buffer: Arc<AudioBufferData>,
		is_loop: bool,
		offset_seconds: f64,
		playback_rate: f64,
		generation: u64,
	},
	StopSource {
		player_id: u64,
		time: f64,
	},
	/// 비프 톤 하나. (원본 BeepPlayer.playTone: OscillatorNode(frequency.setValueAtTime(frequency, start_time)) → GainNode(엔벨로프 자동화 넷) → 출력.
	/// call_time 은 부른 때의 컨텍스트 시계, start_time 은 그보다 앞이 아니다)
	PlayBeep {
		periodic_wave: Arc<PeriodicWave>,
		frequency: f32,
		gain_automation_list: [ParamAutomation; 4],
		call_time: f64,
		start_time: f64,
		stop_time: f64,
	},
	Suspend,
	Resume,
}

//==============================================================================
/// 소리 스레드와 나누는 값. (컨텍스트 시계와 표본율)
/// - 시계는 그린 프레임 수 / 표본율의 f64 비트다. (원본 currentTime, 장치가 바뀌어 표본율이 달라지면 프레임 수를 그 비로 옮겨 잇는다)
//==============================================================================
pub(crate) struct MixerShared {
	rendered_seconds_bits: AtomicU64,
	sample_rate_bits: AtomicU64,
}

impl MixerShared {
	//==============================================================================
	/// 생성. (시계는 0, 표본율은 sample_rate)
	//==============================================================================
	pub(crate) fn new(sample_rate: f64) -> MixerShared {
		MixerShared {
			rendered_seconds_bits: AtomicU64::new(0f64.to_bits()),
			sample_rate_bits: AtomicU64::new(sample_rate.to_bits()),
		}
	}

	//==============================================================================
	/// 그린 시간. (초, 원본 currentTime)
	//==============================================================================
	pub(crate) fn get_rendered_seconds(&self) -> f64 {
		f64::from_bits(self.rendered_seconds_bits.load(Ordering::Acquire))
	}

	//==============================================================================
	/// 표본율. (원본 sampleRate, 장치의 표본율)
	//==============================================================================
	pub(crate) fn get_sample_rate(&self) -> f64 {
		f64::from_bits(self.sample_rate_bits.load(Ordering::Acquire))
	}

	//==============================================================================
	/// 그린 시간 쓰기. (초, 소리 스레드가 퀀텀을 그릴 때마다)
	//==============================================================================
	fn set_rendered_seconds(&self, rendered_seconds: f64) {
		self.rendered_seconds_bits.store(rendered_seconds.to_bits(), Ordering::Release);
	}
}

//==============================================================================
/// 시간을 표본 자리로. (크롬 audio_utilities::TimeToSampleFrame, 1024 배로 셈해 반올림한 뒤 가장 가까운 정수)
//==============================================================================
fn time_to_sample_frame(time: f64, sample_rate: f64) -> f64 {
	const OVERSAMPLE_FACTOR: f64 = 1024.0;
	let frame = (time * sample_rate * OVERSAMPLE_FACTOR).round() / OVERSAMPLE_FACTOR;
	frame.round()
}

//==============================================================================
/// 시간을 표본 자리로 올림. (TimeToSampleFrame 의 kRoundUp, 예약 정지)
//==============================================================================
pub(super) fn time_to_sample_frame_rounded_up(time: f64, sample_rate: f64) -> u64 {
	const OVERSAMPLE_FACTOR: f64 = 1024.0;
	let frame = (time * sample_rate * OVERSAMPLE_FACTOR).round() / OVERSAMPLE_FACTOR;
	frame.ceil().max(0.0) as u64
}

//==============================================================================
/// AudioBufferSourceNode. (크롬 AudioBufferSourceHandler, 루프는 버퍼 전체, 빠르기는 0 보다 크다)
//==============================================================================
struct BufferSource {
	audio_buffer: Arc<AudioBufferData>,
	virtual_read_index: f64,
	computed_playback_rate: f64,
	is_looping: bool,
	is_finished: bool,
	/// stop(when) 의 시각. (없으면 끝까지)
	end_time: Option<f64>,
}

impl BufferSource {
	//==============================================================================
	/// 생성. (start(0, offset): ClampGrainParameters 와 ComputePlaybackRate)
	//==============================================================================
	fn new(audio_buffer: Arc<AudioBufferData>, is_loop: bool, offset_seconds: f64, playback_rate: f64, context_sample_rate: f64) -> BufferSource {
		let buffer_sample_rate = audio_buffer.sample_rate;
		let buffer_duration = audio_buffer.get_length() as f64 / buffer_sample_rate;
		let grain_offset = offset_seconds.clamp(0.0, buffer_duration);
		// playbackRate 는 AudioParam 이라 f32 로 든다.
		let playback_rate = playback_rate as f32 as f64;
		let virtual_read_index = if playback_rate == 1.0 { time_to_sample_frame(grain_offset, buffer_sample_rate) } else { grain_offset * buffer_sample_rate };
		let computed_playback_rate = (buffer_sample_rate / context_sample_rate * playback_rate).clamp(-MAXIMUM_PLAYBACK_RATE, MAXIMUM_PLAYBACK_RATE);
		BufferSource {
			audio_buffer,
			virtual_read_index,
			computed_playback_rate,
			is_looping: is_loop,
			is_finished: false,
			end_time: None,
		}
	}

	//==============================================================================
	/// 퀀텀 하나 그리기. (RenderFromBuffer, 채널 수를 돌려준다. 끝나면 is_finished)
	//==============================================================================
	fn render(&mut self, destination: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], frame_count: usize) -> usize {
		let channel_count = self.audio_buffer.channel_list.len().min(2);
		for channel in destination.iter_mut().take(channel_count) {
			channel.fill(0.0);
		}
		let buffer_length = self.audio_buffer.get_length();
		let virtual_start_frame = 0.0;
		let virtual_end_frame = buffer_length as f64;
		let virtual_delta_frames = virtual_end_frame - virtual_start_frame;
		let computed_playback_rate = self.computed_playback_rate;
		if computed_playback_rate.abs() > virtual_delta_frames {
			return channel_count;
		}
		let mut virtual_read_index = self.virtual_read_index;
		let mut frames_to_process = frame_count;
		if self.is_looping && virtual_read_index >= virtual_end_frame {
			virtual_read_index = virtual_start_frame.min((buffer_length - 1) as f64);
			self.virtual_read_index = virtual_read_index;
		}
		let mut is_stopping_this_quantum = false;
		if !self.is_looping && virtual_read_index >= buffer_length as f64 {
			virtual_read_index = virtual_read_index.clamp(0.0, buffer_length as f64);
			frames_to_process = 0;
			is_stopping_this_quantum = true;
		}
		let mut write_index = 0usize;
		if frames_to_process > 0 {
			let is_fast_path = computed_playback_rate == 1.0 && virtual_read_index == virtual_read_index.floor() && virtual_delta_frames == virtual_delta_frames.floor() && virtual_end_frame == virtual_end_frame.floor();
			let (next_write_index, next_virtual_read_index) = if is_fast_path {
				self.process_fast_path(destination, channel_count, virtual_delta_frames, virtual_end_frame, frames_to_process, virtual_read_index)
			}
			else {
				self.process_interpolated_path(destination, channel_count, virtual_start_frame, virtual_delta_frames, virtual_end_frame, frames_to_process, virtual_read_index)
			};
			write_index = next_write_index;
			virtual_read_index = next_virtual_read_index;
		}
		if is_stopping_this_quantum {
			for channel in destination.iter_mut().take(channel_count) {
				channel[write_index..frame_count].fill(0.0);
			}
			self.is_finished = true;
		}
		self.virtual_read_index = virtual_read_index;
		channel_count
	}

	//==============================================================================
	/// 그대로 베끼기. (ProcessFastPath)
	//==============================================================================
	fn process_fast_path(&mut self, destination: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], channel_count: usize, virtual_delta_frames: f64, virtual_end_frame: f64, frames_to_process: usize, virtual_read_index: f64) -> (usize, f64) {
		let buffer_length = self.audio_buffer.get_length();
		let mut read_index = virtual_read_index as usize;
		let delta_frames = virtual_delta_frames as usize;
		let end_frame = virtual_end_frame as usize;
		let mut write_index = 0usize;
		let mut remaining_frame_count = frames_to_process as i64;
		while remaining_frame_count > 0 {
			let frames_to_end = end_frame as i64 - read_index as i64;
			let frames_this_time = frames_to_end.clamp(0, remaining_frame_count) as usize;
			for channel_index in 0..channel_count {
				let source = &self.audio_buffer.channel_list[channel_index];
				destination[channel_index][write_index..write_index + frames_this_time].copy_from_slice(&source[read_index..read_index + frames_this_time]);
			}
			write_index += frames_this_time;
			read_index += frames_this_time;
			remaining_frame_count -= frames_this_time as i64;
			let mut temporary_read_index = read_index as f64;
			if delta_frames == 0 {
				self.is_finished = true;
				break;
			}
			if temporary_read_index >= end_frame as f64 {
				if !self.is_looping {
					for channel in destination.iter_mut().take(channel_count) {
						channel[write_index..].fill(0.0);
					}
					self.is_finished = true;
					read_index = read_index.min(buffer_length);
					break;
				}
				let overflow = temporary_read_index - end_frame as f64;
				temporary_read_index = end_frame as f64 - delta_frames as f64 + overflow % delta_frames as f64;
			}
			read_index = temporary_read_index as usize;
		}
		(write_index, read_index as f64)
	}

	//==============================================================================
	/// 선형 보간. (ProcessInterpolatedPath, 빠르기가 0 보다 큰 쪽)
	//==============================================================================
	#[allow(clippy::too_many_arguments)]
	fn process_interpolated_path(&mut self, destination: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], channel_count: usize, virtual_start_frame: f64, virtual_delta_frames: f64, virtual_end_frame: f64, frames_to_process: usize, mut virtual_read_index: f64) -> (usize, f64) {
		let buffer_length = self.audio_buffer.get_length();
		let maximum_index = buffer_length - 1;
		let computed_playback_rate = self.computed_playback_rate;
		let mut write_index = 0usize;
		for _ in 0..frames_to_process {
			let read_index_floor = virtual_read_index as usize;
			let interpolation_factor = virtual_read_index - read_index_floor as f64;
			let mut read_index = read_index_floor;
			let mut read_index2 = read_index + 1;
			if self.is_looping && (read_index as f64) < virtual_end_frame && read_index2 as f64 >= virtual_end_frame {
				let wrapped_index = if virtual_delta_frames >= 1.0 {
					virtual_read_index + 1.0 - virtual_delta_frames
				}
				else if virtual_delta_frames > 0.0 {
					virtual_start_frame + (virtual_read_index + 1.0 - virtual_start_frame) % virtual_delta_frames
				}
				else {
					virtual_start_frame
				};
				read_index2 = if wrapped_index >= 0.0 { wrapped_index as usize } else { 0 };
			}
			else if read_index2 >= buffer_length {
				read_index2 = read_index;
			}
			read_index = read_index.min(maximum_index);
			read_index2 = read_index2.min(maximum_index);
			for channel_index in 0..channel_count {
				let source = &self.audio_buffer.channel_list[channel_index];
				let sample = if read_index == read_index2 && read_index >= 1 {
					let sample1 = source[read_index - 1] as f64;
					let sample2 = source[read_index] as f64;
					sample2 + (sample2 - sample1) * interpolation_factor
				}
				else {
					let sample1 = source[read_index] as f64;
					let sample2 = source[read_index2] as f64;
					(1.0 - interpolation_factor) * sample1 + interpolation_factor * sample2
				};
				destination[channel_index][write_index] = sample as f32;
			}
			write_index += 1;
			virtual_read_index += computed_playback_rate;
			if virtual_delta_frames <= 0.0 {
				self.is_finished = true;
				break;
			}
			if virtual_read_index >= virtual_end_frame {
				if !self.is_looping {
					for channel in destination.iter_mut().take(channel_count) {
						channel[write_index..].fill(0.0);
					}
					self.is_finished = true;
					virtual_read_index = virtual_read_index.min(buffer_length as f64);
					break;
				}
				let overflow = virtual_read_index - virtual_end_frame;
				virtual_read_index = virtual_end_frame - virtual_delta_frames + overflow % virtual_delta_frames;
			}
		}
		(write_index, virtual_read_index)
	}
}

//==============================================================================
/// 좌우 이득. (크롬 StereoPanner::PanToTargetValue, f64 로 셈해 f32 로)
//==============================================================================
fn get_pan_gains(pan: f32, channel_count: usize) -> (f32, f32) {
	let target_pan = pan.clamp(-1.0, 1.0) as f64;
	let pan_radian = if channel_count == 1 {
		(target_pan * 0.5 + 0.5) * std::f64::consts::FRAC_PI_2
	}
	else if target_pan <= 0.0 {
		(target_pan + 1.0) * std::f64::consts::FRAC_PI_2
	}
	else {
		target_pan * std::f64::consts::FRAC_PI_2
	};
	(pan_radian.cos() as f32, pan_radian.sin() as f32)
}

//==============================================================================
/// 플레이어. (원본 AudioPlayer 의 GainNode 와 지금 소스, 좌우 노드)
//==============================================================================
struct PlayerNode {
	player_id: u64,
	gain: AudioParam,
	/// 원본 gain.value 가 읽는 내재 값을 플레이어 손잡이에 나눠 준다.
	gain_value: Arc<AtomicU32>,
	/// StereoPannerNode 의 값. (0 이 아닌 좌우를 처음 걸 때 생기고 그 뒤로 남는다)
	pan: Option<f32>,
	/// PannerNode. (공간 소리를 처음 걸 때 생기고 그 뒤로 남는다, 초안)
	panner: Option<Box<PannerNode>>,
	output_bus_id: Option<u64>,
	source: Option<(BufferSource, u64)>,
	ended_generation: Arc<AtomicU64>,
}

//==============================================================================
/// 버스. (원본 AudioBus 의 GainNode)
//==============================================================================
struct BusNode {
	bus_id: u64,
	gain: AudioParam,
	input: [[f32; RENDER_QUANTUM_FRAMES]; 2],
	/// 이 퀀텀에 소리 나는 입력이 있었는지. (없으면 크롬처럼 자동화만 흘린다)
	has_sound_input: bool,
}

//==============================================================================
/// GainNode 의 gain 을 퀀텀에 걸기. (GainHandler::Process: 자동화가 있으면 표본마다, 없으면 하나, 0 이면 소리 없음. 소리가 나면 참)
//==============================================================================
fn apply_gain_param(gain: &mut AudioParam, quantum: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], channel_count: usize, gain_value_list: &mut [f32; RENDER_QUANTUM_FRAMES], current_frame: u64, sample_rate: f64) -> bool {
	if gain.has_sample_accurate_values(current_frame, sample_rate, RENDER_QUANTUM_FRAMES) {
		gain.calculate_sample_accurate_values(gain_value_list, current_frame, sample_rate);
		for channel in quantum.iter_mut().take(channel_count) {
			for (sample, gain_value) in channel.iter_mut().zip(gain_value_list.iter()) {
				*sample *= *gain_value;
			}
		}
		return true;
	}
	let gain_value = gain.calculate_quantum_value(current_frame, sample_rate, RENDER_QUANTUM_FRAMES);
	if gain_value == 0.0 {
		for channel in quantum.iter_mut().take(channel_count) {
			channel.fill(0.0);
		}
		return false;
	}
	for channel in quantum.iter_mut().take(channel_count) {
		for sample in channel.iter_mut() {
			*sample *= gain_value;
		}
	}
	true
}

/// GainNode 의 gain 기본값과 범위. (1, f32 전체)
fn create_gain_param() -> AudioParam {
	AudioParam::new(1.0, f32::MIN, f32::MAX)
}

//==============================================================================
/// 비프. (OscillatorNode → GainNode → 출력, 오실레이터가 끝나면 뺀다)
//==============================================================================
struct BeepVoice {
	oscillator: OscillatorNode,
	gain: AudioParam,
}

//==============================================================================
/// 소리 스레드의 믹서.
//==============================================================================
pub(crate) struct MixerRenderer {
	command_receiver: Receiver<MixerCommand>,
	shared: Arc<MixerShared>,
	sample_rate: f64,
	/// 그린 프레임 수. (원본 currentSampleFrame, 다음 퀀텀의 첫 프레임)
	current_frame: u64,
	is_suspended: bool,
	player_list: Vec<PlayerNode>,
	bus_list: Vec<BusNode>,
	beep_list: Vec<BeepVoice>,
	//// 리스너. (컨텍스트에 하나, 초안)
	listener: ListenerState,
	/// 그린 퀀텀. (출력 L, R 과 다음에 낼 자리)
	output_quantum: [[f32; RENDER_QUANTUM_FRAMES]; 2],
	output_position: usize,
	source_quantum: [[f32; RENDER_QUANTUM_FRAMES]; 2],
	gain_value_list: [f32; RENDER_QUANTUM_FRAMES],
}

impl MixerRenderer {
	//==============================================================================
	/// 생성.
	//==============================================================================
	pub(crate) fn new(command_receiver: Receiver<MixerCommand>, shared: Arc<MixerShared>, sample_rate: f64) -> MixerRenderer {
		MixerRenderer {
			command_receiver,
			shared,
			sample_rate,
			current_frame: 0,
			is_suspended: false,
			player_list: Vec::with_capacity(256),
			bus_list: Vec::with_capacity(16),
			beep_list: Vec::with_capacity(16),
			listener: ListenerState::new(),
			output_quantum: [[0.0; RENDER_QUANTUM_FRAMES]; 2],
			output_position: RENDER_QUANTUM_FRAMES,
			source_quantum: [[0.0; RENDER_QUANTUM_FRAMES]; 2],
			gain_value_list: [0.0; RENDER_QUANTUM_FRAMES],
		}
	}

	//==============================================================================
	/// 받은 명령 처리하기.
	//==============================================================================
	fn receive_commands(&mut self) {
		while let Ok(command) = self.command_receiver.try_recv() {
			match command {
				MixerCommand::AddBus { bus_id } => {
					self.bus_list.push(BusNode { bus_id, gain: create_gain_param(), input: [[0.0; RENDER_QUANTUM_FRAMES]; 2], has_sound_input: false });
				}
				MixerCommand::RemoveBus { bus_id } => {
					self.bus_list.retain(|bus| bus.bus_id != bus_id);
				}
				MixerCommand::BusGainAutomation { bus_id, automation } => {
					let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
					if let Some(bus) = self.bus_list.iter_mut().find(|bus| bus.bus_id == bus_id) {
						bus.gain.apply_automation(automation, current_frame, sample_rate);
					}
				}
				MixerCommand::AddPlayer { player_id, output_bus_id, ended_generation, gain_value } => {
					self.player_list.push(PlayerNode { player_id, gain: create_gain_param(), gain_value, pan: None, panner: None, output_bus_id, source: None, ended_generation });
				}
				MixerCommand::RemovePlayer { player_id } => {
					self.player_list.retain(|player| player.player_id != player_id);
				}
				MixerCommand::PlayerGainAutomation { player_id, automation } => {
					let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
					if let Some(player) = self.find_player(player_id) {
						player.gain.apply_automation(automation, current_frame, sample_rate);
						player.gain_value.store(player.gain.get_intrinsic_value().to_bits(), Ordering::Release);
					}
				}
				MixerCommand::SetPlayerPan { player_id, pan } => {
					if let Some(player) = self.find_player(player_id) {
						if player.pan.is_some() || pan != 0.0 {
							player.pan = Some(pan);
						}
					}
				}
				MixerCommand::SetPlayerPanner { player_id, settings } => {
					if let Some(player) = self.find_player(player_id) {
						match player.panner.as_mut() {
							Some(panner) => {
								panner.set_settings(settings);
							}
							None => {
								player.panner = Some(Box::new(PannerNode::new(settings)));
							}
						}
					}
				}
				MixerCommand::PlayerPannerAutomation { player_id, param, automation } => {
					let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
					if let Some(player) = self.find_player(player_id) {
						if let Some(panner) = player.panner.as_mut() {
							panner.apply_automation(param, automation, current_frame, sample_rate);
						}
					}
				}
				MixerCommand::ListenerAutomation { param, automation } => {
					let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
					self.listener.apply_automation(param, automation, current_frame, sample_rate);
				}
				MixerCommand::StartSource { player_id, audio_buffer, is_loop, offset_seconds, playback_rate, generation } => {
					let sample_rate = self.sample_rate;
					if let Some(player) = self.find_player(player_id) {
						if let Some((_, previous_generation)) = player.source.take() {
							player.ended_generation.fetch_max(previous_generation, Ordering::Release);
						}
						if audio_buffer.get_length() > 0 {
							player.source = Some((BufferSource::new(audio_buffer, is_loop, offset_seconds, playback_rate, sample_rate), generation));
						}
						else {
							player.ended_generation.fetch_max(generation, Ordering::Release);
						}
					}
				}
				MixerCommand::StopSource { player_id, time } => {
					if let Some(player) = self.find_player(player_id) {
						if let Some((source, _)) = player.source.as_mut() {
							source.end_time = Some(time.max(0.0));
						}
					}
				}
				MixerCommand::PlayBeep { periodic_wave, frequency, gain_automation_list, call_time, start_time, stop_time } => {
					// 자동화 시각은 원본 메인 스레드처럼 부른 때의 시계로 당긴다. (그리기 전에 지금 퀀텀으로 한 번 더)
					let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
					let mut oscillator = OscillatorNode::new(periodic_wave, sample_rate, start_time, stop_time);
					oscillator.frequency.apply_automation_at_call_time(ParamAutomation::SetValueAtTime { value: frequency, time: start_time }, call_time, current_frame, sample_rate);
					let mut gain = create_gain_param();
					for automation in gain_automation_list {
						gain.apply_automation_at_call_time(automation, call_time, current_frame, sample_rate);
					}
					self.beep_list.push(BeepVoice { oscillator, gain });
				}
				MixerCommand::Suspend => {
					self.is_suspended = true;
				}
				MixerCommand::Resume => {
					self.is_suspended = false;
				}
			}
		}
	}

	//==============================================================================
	/// 번호로 플레이어 노드 찾기. (없으면 None)
	//==============================================================================
	fn find_player(&mut self, player_id: u64) -> Option<&mut PlayerNode> {
		self.player_list.iter_mut().find(|player| player.player_id == player_id)
	}

	//==============================================================================
	/// 퀀텀 하나 그리기. (플레이어 → 버스 → 출력, 비프 → 출력)
	//==============================================================================
	fn render_quantum(&mut self) {
		for channel in self.output_quantum.iter_mut() {
			channel.fill(0.0);
		}
		for bus in self.bus_list.iter_mut() {
			for channel in bus.input.iter_mut() {
				channel.fill(0.0);
			}
		}
		for bus in self.bus_list.iter_mut() {
			bus.has_sound_input = false;
		}
		let (current_frame, sample_rate) = (self.current_frame, self.sample_rate);
		let quantum_end_frame = current_frame + RENDER_QUANTUM_FRAMES as u64;
		// 리스너 상태. (그리기 전, 크롬 HandlePreRenderTasks)
		self.listener.update_state(current_frame, sample_rate);
		for player in self.player_list.iter_mut() {
			// 소스: 예약 정지 시각까지 그리고, 그 퀀텀에 끝낸다.
			let mut channel_count = 0;
			if let Some((source, generation)) = player.source.as_mut() {
				let end_frame = source.end_time.map(|end_time| time_to_sample_frame_rounded_up(end_time, sample_rate));
				let is_stopped_before = end_frame.map(|end_frame| end_frame <= current_frame).unwrap_or(false);
				if !is_stopped_before {
					let frame_count = match end_frame {
						Some(end_frame) if end_frame < quantum_end_frame => {
							(end_frame - current_frame) as usize
						}
						_ => {
							RENDER_QUANTUM_FRAMES
						}
					};
					channel_count = source.render(&mut self.source_quantum, frame_count);
				}
				let is_end_reached = end_frame.map(|end_frame| end_frame <= quantum_end_frame).unwrap_or(false);
				if source.is_finished || is_end_reached {
					player.ended_generation.fetch_max(*generation, Ordering::Release);
					player.source = None;
				}
			}
			// GainNode. (소스가 없으면 자동화만 흘린다)
			let has_sound = if channel_count == 0 {
				player.gain.calculate_sample_accurate_values(&mut self.gain_value_list, current_frame, sample_rate);
				false
			}
			else {
				apply_gain_param(&mut player.gain, &mut self.source_quantum, channel_count, &mut self.gain_value_list, current_frame, sample_rate)
			};
			player.gain_value.store(player.gain.get_intrinsic_value().to_bits(), Ordering::Release);
			if !has_sound {
				// PannerNode 는 조용한 입력으로 돈다. (AudioParam 은 흐르고, 침묵을 넘기기 전까지는 셈한다. HRTF 는 꼬리와 늦음 동안 소리가 난다)
				let has_panner_output = match player.panner.as_mut() {
					Some(panner) => {
						panner.process(&mut self.source_quantum, channel_count.max(1), true, &mut self.listener, current_frame, sample_rate)
					}
					None => {
						false
					}
				};
				if has_panner_output {
					mix_player_output(&mut self.bus_list, &mut self.output_quantum, &self.source_quantum, player.output_bus_id, 2);
				}
				continue;
			}
			// StereoPannerNode. (모노도 두 채널이 된다)
			let output_channel_count = match player.pan {
				Some(pan) => {
					let (gain_left, gain_right) = get_pan_gains(pan, channel_count);
					let [left, right] = &mut self.source_quantum;
					if channel_count == 1 {
						for frame_index in 0..RENDER_QUANTUM_FRAMES {
							let input = left[frame_index];
							right[frame_index] = input * gain_right;
							left[frame_index] = input * gain_left;
						}
					}
					else if pan.clamp(-1.0, 1.0) <= 0.0 {
						for frame_index in 0..RENDER_QUANTUM_FRAMES {
							let input_right = right[frame_index];
							left[frame_index] += input_right * gain_left;
							right[frame_index] = input_right * gain_right;
						}
					}
					else {
						for frame_index in 0..RENDER_QUANTUM_FRAMES {
							let input_left = left[frame_index];
							right[frame_index] += input_left * gain_right;
							left[frame_index] = input_left * gain_left;
						}
					}
					2
				}
				None => {
					channel_count
				}
			};
			// PannerNode. (모노도 두 채널이 된다)
			let output_channel_count = match player.panner.as_mut() {
				Some(panner) => {
					panner.process(&mut self.source_quantum, output_channel_count, false, &mut self.listener, current_frame, sample_rate);
					2
				}
				None => {
					output_channel_count
				}
			};
			// 내보낼 곳에 섞기. (모노는 양쪽에 같게)
			mix_player_output(&mut self.bus_list, &mut self.output_quantum, &self.source_quantum, player.output_bus_id, output_channel_count);
		}
		for bus in self.bus_list.iter_mut() {
			if !bus.has_sound_input {
				bus.gain.calculate_sample_accurate_values(&mut self.gain_value_list, current_frame, sample_rate);
				continue;
			}
			if apply_gain_param(&mut bus.gain, &mut bus.input, 2, &mut self.gain_value_list, current_frame, sample_rate) {
				mix_into(&mut self.output_quantum, &bus.input, 2);
			}
		}
		for beep in self.beep_list.iter_mut() {
			// OscillatorNode → GainNode. (오실레이터가 소리를 내지 않는 동안은 자동화만 흘린다) 모노라 출력 양쪽에 같게.
			if !beep.oscillator.process(&mut self.source_quantum[0], current_frame, sample_rate) {
				beep.gain.calculate_sample_accurate_values(&mut self.gain_value_list, current_frame, sample_rate);
				continue;
			}
			if apply_gain_param(&mut beep.gain, &mut self.source_quantum, 1, &mut self.gain_value_list, current_frame, sample_rate) {
				mix_into(&mut self.output_quantum, &self.source_quantum, 1);
			}
		}
		self.beep_list.retain(|beep| !beep.oscillator.is_finished());
		self.current_frame += RENDER_QUANTUM_FRAMES as u64;
		self.shared.set_rendered_seconds(self.current_frame as f64 / self.sample_rate);
	}

	//==============================================================================
	/// 장치가 바뀌어 표본율이 달라졌을 때. (새로 트는 소스부터 그 표본율로 빠르기를 셈한다)
	//==============================================================================
	pub(crate) fn set_sample_rate(&mut self, sample_rate: f64) {
		if self.sample_rate > 0.0 && sample_rate != self.sample_rate {
			self.current_frame = (self.current_frame as f64 * sample_rate / self.sample_rate).round() as u64;
		}
		self.sample_rate = sample_rate;
		self.shared.sample_rate_bits.store(sample_rate.to_bits(), Ordering::Release);
	}

	//==============================================================================
	/// 장치 버퍼 채우기. (끼워 넣은 channel_count 채널, 앞 두 채널에 L, R 과 나머지 0, 모노 장치는 0.5 * (L + R). -1 ~ 1 로 자른다)
	//==============================================================================
	pub(crate) fn render_interleaved(&mut self, output: &mut [f32], channel_count: usize) {
		if channel_count == 0 {
			return;
		}
		for frame in output.chunks_mut(channel_count) {
			if self.output_position >= RENDER_QUANTUM_FRAMES {
				self.receive_commands();
				if self.is_suspended {
					frame.fill(0.0);
					continue;
				}
				self.render_quantum();
				self.output_position = 0;
			}
			let left = self.output_quantum[0][self.output_position].clamp(-1.0, 1.0);
			let right = self.output_quantum[1][self.output_position].clamp(-1.0, 1.0);
			self.output_position += 1;
			if channel_count == 1 {
				frame[0] = (0.5 * (self.output_quantum[0][self.output_position - 1] + self.output_quantum[1][self.output_position - 1])).clamp(-1.0, 1.0);
				continue;
			}
			frame[0] = left;
			frame[1] = right;
			for sample in frame.iter_mut().skip(2) {
				*sample = 0.0;
			}
		}
	}
}

//==============================================================================
/// 플레이어 출력을 내보낼 곳에 섞기. (버스가 있으면 그 버스 입력에 소리를 냈다고 표시하고, 없는 버스면 버린다. 없으면 출력)
///
/// # 인자
/// - `bus_list`: 버스들.
/// - `output_quantum`: 출력 퀀텀.
/// - `source_quantum`: 플레이어 출력.
/// - `output_bus_id`: 내보낼 버스.
/// - `channel_count`: 플레이어 출력 채널 수.
//==============================================================================
fn mix_player_output(bus_list: &mut [BusNode], output_quantum: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], source_quantum: &[[f32; RENDER_QUANTUM_FRAMES]; 2], output_bus_id: Option<u64>, channel_count: usize) {
	let target = match output_bus_id {
		Some(bus_id) => {
			match bus_list.iter_mut().find(|bus| bus.bus_id == bus_id) {
				Some(bus) => {
					bus.has_sound_input = true;
					&mut bus.input
				}
				None => {
					return;
				}
			}
		}
		None => {
			output_quantum
		}
	};
	mix_into(target, source_quantum, channel_count);
}

//==============================================================================
/// 입력 섞기. (모노 입력은 스피커 규칙으로 양쪽에 같게 올린다)
//==============================================================================
fn mix_into(target: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2], source: &[[f32; RENDER_QUANTUM_FRAMES]; 2], source_channel_count: usize) {
	for frame_index in 0..RENDER_QUANTUM_FRAMES {
		if source_channel_count == 1 {
			target[0][frame_index] += source[0][frame_index];
			target[1][frame_index] += source[0][frame_index];
		}
		else {
			target[0][frame_index] += source[0][frame_index];
			target[1][frame_index] += source[1][frame_index];
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::mpsc::channel;

	//==============================================================================
	/// 시험 버퍼. (채널 목록과 표본율)
	//==============================================================================
	fn create_buffer(channel_list: Vec<Vec<f32>>, sample_rate: f64) -> Arc<AudioBufferData> {
		Arc::new(AudioBufferData { sample_rate, channel_list })
	}

	//==============================================================================
	/// 시험 렌더러. (명령 보내는 쪽, 렌더러, 나누는 값)
	//==============================================================================
	fn create_renderer(sample_rate: f64) -> (std::sync::mpsc::Sender<MixerCommand>, MixerRenderer, Arc<MixerShared>) {
		let (command_sender, command_receiver) = channel();
		let shared = Arc::new(MixerShared::new(sample_rate));
		(command_sender, MixerRenderer::new(command_receiver, shared.clone(), sample_rate), shared)
	}

	//==============================================================================
	/// 같은 표본율, 빠르기 1 은 그대로 베끼고 끝에서 멈춘다. (ProcessFastPath)
	//==============================================================================
	#[test]
	fn buffer_source_copies_and_finishes() {
		let buffer = create_buffer(vec![(0..200).map(|index| index as f32 / 200.0).collect()], 48000.0);
		let mut source = BufferSource::new(buffer, false, 0.0, 1.0, 48000.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		assert_eq!(source.render(&mut quantum, RENDER_QUANTUM_FRAMES), 1);
		assert_eq!(quantum[0][5], 5.0 / 200.0);
		assert!(!source.is_finished);
		source.render(&mut quantum, RENDER_QUANTUM_FRAMES);
		assert_eq!(quantum[0][71], 199.0 / 200.0);
		assert_eq!(quantum[0][72], 0.0);
		assert!(source.is_finished);
	}

	//==============================================================================
	/// 표본율이 다르면 선형 보간하고, 끝 표본은 선형으로 넘겨 그린다. (ProcessInterpolatedPath)
	//==============================================================================
	#[test]
	fn buffer_source_interpolates_linearly() {
		let buffer = create_buffer(vec![vec![0.0, 1.0, 2.0, 3.0]], 44100.0);
		let mut source = BufferSource::new(buffer, false, 0.0, 1.0, 88200.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		source.render(&mut quantum, RENDER_QUANTUM_FRAMES);
		assert_eq!(&quantum[0][0..8], &[0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]);
		assert_eq!(quantum[0][8], 0.0);
		assert!(source.is_finished);
	}

	//==============================================================================
	/// 루프는 버퍼 전체를 되풀이하고, 시작 위치는 1024 배 오버샘플로 반올림한다.
	//==============================================================================
	#[test]
	fn buffer_source_loops_from_offset() {
		let buffer = create_buffer(vec![vec![0.0, 1.0, 2.0, 3.0]], 44100.0);
		let mut source = BufferSource::new(buffer, true, 2.0 / 44100.0, 1.0, 44100.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		source.render(&mut quantum, RENDER_QUANTUM_FRAMES);
		assert_eq!(&quantum[0][0..7], &[2.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0]);
		assert!(!source.is_finished);
		assert_eq!(time_to_sample_frame(1.0 / 3.0, 44100.0), 14700.0);
	}

	//==============================================================================
	/// 좌우: 모노는 가운데에서 양쪽 cos(π/4), 스테레오는 0 이면 그대로. (크롬 PanToTargetValue)
	//==============================================================================
	#[test]
	fn stereo_panner_matches_chromium() {
		let (gain_left, gain_right) = get_pan_gains(0.0, 1);
		assert_eq!(gain_left, (std::f64::consts::FRAC_PI_4).cos() as f32);
		assert_eq!(gain_right, (std::f64::consts::FRAC_PI_4).sin() as f32);
		let (gain_left, gain_right) = get_pan_gains(0.0, 2);
		assert!(gain_left.abs() < 1e-7 && gain_right == 1.0);
		let (gain_left, gain_right) = get_pan_gains(1.0, 2);
		assert!((gain_left).abs() < 1e-7 && gain_right == 1.0);
		let (gain_left, gain_right) = get_pan_gains(-0.5, 2);
		assert_eq!(gain_left, (0.25 * std::f64::consts::PI).cos() as f32);
		assert_eq!(gain_right, (0.25 * std::f64::consts::PI).sin() as f32);
	}

	//==============================================================================
	/// 그래프: 플레이어(음량, 좌우) → 버스(음량) → 출력. 모노는 양쪽에 올리고, 멈춘 동안은 그리지 않는다.
	//==============================================================================
	#[test]
	fn mixer_graph_applies_gain_pan_and_bus() {
		let (command_sender, mut renderer, shared) = create_renderer(44100.0);
		let ended_generation = Arc::new(AtomicU64::new(0));
		command_sender.send(MixerCommand::AddBus { bus_id: 1 }).unwrap();
		command_sender.send(MixerCommand::BusGainAutomation { bus_id: 1, automation: ParamAutomation::SetValue { value: 0.5 } }).unwrap();
		command_sender.send(MixerCommand::AddPlayer { player_id: 2, output_bus_id: Some(1), ended_generation: ended_generation.clone(), gain_value: Arc::new(AtomicU32::new(0)) }).unwrap();
		command_sender.send(MixerCommand::PlayerGainAutomation { player_id: 2, automation: ParamAutomation::SetValue { value: 0.8 } }).unwrap();
		command_sender.send(MixerCommand::StartSource { player_id: 2, audio_buffer: create_buffer(vec![vec![0.5; 300]], 44100.0), is_loop: false, offset_seconds: 0.0, playback_rate: 1.0, generation: 1 }).unwrap();
		let mut output = vec![0.0f32; 2 * 4];
		renderer.render_interleaved(&mut output, 2);
		assert_eq!(output[0], 0.5 * 0.8 * 0.5);
		assert_eq!(output[1], 0.5 * 0.8 * 0.5);
		// 좌우를 걸면 모노도 두 채널 식으로.
		command_sender.send(MixerCommand::SetPlayerPan { player_id: 2, pan: 1.0 }).unwrap();
		let mut rest = vec![0.0f32; 2 * 124];
		renderer.render_interleaved(&mut rest, 2);
		let mut next = vec![0.0f32; 2 * 2];
		renderer.render_interleaved(&mut next, 2);
		let (gain_left, gain_right) = get_pan_gains(1.0, 1);
		assert_eq!(next[0], 0.5 * 0.8 * gain_left * 0.5);
		assert_eq!(next[1], 0.5 * 0.8 * gain_right * 0.5);
		assert_eq!(ended_generation.load(Ordering::Acquire), 0);
		// 끝까지 그리면 끝난 세대가 올라간다.
		let mut tail = vec![0.0f32; 2 * 512];
		renderer.render_interleaved(&mut tail, 2);
		assert_eq!(ended_generation.load(Ordering::Acquire), 1);
		// 멈춘 동안은 0 을 내고 시계도 선다.
		let rendered_seconds = shared.get_rendered_seconds();
		assert!((rendered_seconds - 6.0 * 128.0 / 44100.0).abs() < 1e-12);
		command_sender.send(MixerCommand::Suspend).unwrap();
		let mut silent = vec![1.0f32; 2 * 300];
		renderer.render_interleaved(&mut silent, 2);
		assert!(silent.iter().all(|sample| *sample == 0.0));
		assert_eq!(shared.get_rendered_seconds(), rendered_seconds);
	}

	//==============================================================================
	/// 크롬(158) OfflineAudioContext 와 같은 그래프를 그려 견준다. (tests/assets/mixer/chromemixer.cjs 가 찍은 chrome_mixer.json)
	/// - 소스(같은 표본율과 다른 표본율, 빠르기, 루프, 시작 위치, stop(when)), 플레이어 GainNode, StereoPannerNode(모노와 스테레오, 0 포함), 버스 GainNode, 섞기.
	/// - AudioParam 자동화(value 대입, setValueAtTime, linearRamp, exponentialRamp, setTarget, cancel)와 그리는 도중 지금 gain.value 에서 줄여 끄기, 새 음악 올리기.
	/// - 자동화의 exp, pow 는 크롬도 엔진도 운영체제의 수학 라이브러리라 기계마다 끝자리가 다르다. 기대값을 찍은 운영체제와 CPU 가 같으면 제곱합을 1e-6, 다르면 1e-5 (상대) 안으로 본다.
	///   (애플 실리콘 맥 크롬 158 의 기대값과 윈도우 x64 는 줄여 끄는 그래프에서 1.1e-6 다르고, 그 기계의 크롬과는 1e-6 안이다)
	//==============================================================================
	#[test]
	fn mixer_matches_chrome_offline_rendering() {
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/mixer/chrome_mixer.json")).expect("chrome_mixer.json")).expect("json");
		let platform = match std::env::consts::OS {
			"windows" => {
				"win32"
			}
			"macos" => {
				"darwin"
			}
			other => {
				other
			}
		};
		let architecture = if cfg!(target_arch = "aarch64") {
			"arm64"
		}
		else if cfg!(target_arch = "x86_64") {
			"x64"
		}
		else {
			"ia32"
		};
		let is_same_machine = json["platform"].as_str() == Some(platform) && json["architecture"].as_str() == Some(architecture);
		let energy_tolerance = if is_same_machine { 1e-6 } else { 1e-5 };
		let buffer_sample_rate = json["bufferSampleRate"].as_f64().unwrap();
		let buffer_length = json["bufferLength"].as_u64().unwrap() as usize;
		let render_length = json["renderLength"].as_u64().unwrap() as usize;
		let index_list: Vec<usize> = json["index"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as usize).collect();
		let create_channel = |multiplier: usize, modulus: usize, offset: f64, divisor: f64| -> Vec<f32> {
			(0..buffer_length).map(|index| ((((index * multiplier) % modulus) as f64 - offset) / divisor) as f32).collect()
		};
		let stereo_buffer = create_buffer(vec![create_channel(37, 200, 100.0, 250.0), create_channel(53, 160, 80.0, 200.0)], buffer_sample_rate);
		let mono_buffer = create_buffer(vec![create_channel(29, 180, 90.0, 225.0)], buffer_sample_rate);
		let read_automation_list = |automation_list: &serde_json::Value| -> Vec<ParamAutomation> {
			automation_list.as_array().unwrap().iter().map(|automation| {
				let value = automation["value"].as_f64().unwrap_or(0.0) as f32;
				let time = automation["time"].as_f64().unwrap();
				match automation["type"].as_str().unwrap() {
					"setValueAtTime" => ParamAutomation::SetValueAtTime { value, time },
					"linearRamp" => ParamAutomation::LinearRampToValueAtTime { value, time },
					"exponentialRamp" => ParamAutomation::ExponentialRampToValueAtTime { value, time },
					"setTarget" => ParamAutomation::SetTargetAtTime { target: value, time, time_constant: automation["timeConstant"].as_f64().unwrap() },
					_ => ParamAutomation::CancelScheduledValues { time },
				}
			}).collect()
		};
		let case_list = json["cases"].as_array().unwrap();
		assert_eq!(case_list.len(), 9);
		for test_case in case_list {
			let name = test_case["name"].as_str().unwrap();
			let context_rate = test_case["contextRate"].as_f64().unwrap();
			let (command_sender, mut renderer, _) = create_renderer(context_rate);
			let mut bus_id_list: Vec<u64> = Vec::new();
			for (bus_index, bus) in test_case["busList"].as_array().unwrap().iter().enumerate() {
				let bus_id = 100 + bus_index as u64;
				command_sender.send(MixerCommand::AddBus { bus_id }).unwrap();
				if let Some(gain) = bus["gain"].as_f64() {
					command_sender.send(MixerCommand::BusGainAutomation { bus_id, automation: ParamAutomation::SetValue { value: gain as f32 } }).unwrap();
				}
				for automation in read_automation_list(&bus["automation"]) {
					command_sender.send(MixerCommand::BusGainAutomation { bus_id, automation }).unwrap();
				}
				bus_id_list.push(bus_id);
			}
			let player_list = test_case["playerList"].as_array().unwrap();
			let gain_value_list: Vec<Arc<AtomicU32>> = player_list.iter().map(|_| Arc::new(AtomicU32::new(0))).collect();
			let start_player = |player_index: usize| {
				let player = &player_list[player_index];
				let player_id = 200 + player_index as u64;
				let output_bus_id = player["bus"].as_u64().map(|bus_index| bus_id_list[bus_index as usize]);
				command_sender.send(MixerCommand::AddPlayer { player_id, output_bus_id, ended_generation: Arc::new(AtomicU64::new(0)), gain_value: gain_value_list[player_index].clone() }).unwrap();
				if let Some(gain) = player["gain"].as_f64() {
					command_sender.send(MixerCommand::PlayerGainAutomation { player_id, automation: ParamAutomation::SetValue { value: gain as f32 } }).unwrap();
				}
				for automation in read_automation_list(&player["automation"]) {
					command_sender.send(MixerCommand::PlayerGainAutomation { player_id, automation }).unwrap();
				}
				if let Some(pan) = player["pan"].as_f64() {
					// 원본처럼 0 이 아닌 좌우로 노드를 만든 뒤 값을 건다. (0 도 노드가 있는 꼴)
					command_sender.send(MixerCommand::SetPlayerPan { player_id, pan: 1.0 }).unwrap();
					command_sender.send(MixerCommand::SetPlayerPan { player_id, pan: pan as f32 }).unwrap();
				}
				let audio_buffer = if player["channels"].as_str().unwrap() == "stereo" { stereo_buffer.clone() } else { mono_buffer.clone() };
				command_sender.send(MixerCommand::StartSource { player_id, audio_buffer, is_loop: player["loop"].as_bool().unwrap(), offset_seconds: player["offset"].as_f64().unwrap(), playback_rate: player["rate"].as_f64().unwrap(), generation: 1 }).unwrap();
				if let Some(stop_time) = player["stop"].as_f64() {
					command_sender.send(MixerCommand::StopSource { player_id, time: stop_time }).unwrap();
				}
			};
			for (player_index, player) in player_list.iter().enumerate() {
				if player["startTime"].as_f64().unwrap() == 0.0 {
					start_player(player_index);
				}
			}
			let mut output = vec![0.0f32; render_length * 2];
			let mut rendered_frame_count = 0usize;
			for suspend in test_case["suspendList"].as_array().unwrap() {
				let suspend_time = suspend["time"].as_f64().unwrap();
				let suspend_frame = (suspend_time * context_rate).round() as usize;
				renderer.render_interleaved(&mut output[rendered_frame_count * 2..suspend_frame * 2], 2);
				rendered_frame_count = suspend_frame;
				for action in suspend["actions"].as_array().unwrap() {
					let player_index = action["player"].as_u64().unwrap() as usize;
					let player_id = 200 + player_index as u64;
					if action["kind"].as_str().unwrap() == "fadeOut" {
						// 원본 fadeOutEffects, fadeOutMusic: 지금 gain.value 에서 0 으로, 끝나고 조금 뒤 stop.
						let current_gain = f32::from_bits(gain_value_list[player_index].load(Ordering::Acquire));
						let fade = action["fade"].as_f64().unwrap();
						for automation in [ParamAutomation::CancelScheduledValues { time: suspend_time }, ParamAutomation::SetValueAtTime { value: current_gain, time: suspend_time }, ParamAutomation::LinearRampToValueAtTime { value: 0.0, time: suspend_time + fade }] {
							command_sender.send(MixerCommand::PlayerGainAutomation { player_id, automation }).unwrap();
						}
						command_sender.send(MixerCommand::StopSource { player_id, time: suspend_time + fade + action["margin"].as_f64().unwrap() }).unwrap();
					}
					else {
						start_player(player_index);
					}
				}
			}
			renderer.render_interleaved(&mut output[rendered_frame_count * 2..], 2);
			for channel_index in 0..2 {
				let value_list = test_case["values"][channel_index].as_array().unwrap();
				for (index, value) in index_list.iter().zip(value_list.iter()) {
					let expected = value.as_f64().unwrap();
					let actual = output[index * 2 + channel_index] as f64;
					assert!((actual - expected).abs() < 1e-6, "{} 채널 {} 자리 {}: {} / {}", name, channel_index, index, actual, expected);
				}
				let energy: f64 = (0..render_length).map(|index| (output[index * 2 + channel_index] as f64).powi(2)).sum();
				let expected_energy = test_case["energy"][channel_index].as_f64().unwrap();
				assert!((energy - expected_energy).abs() <= expected_energy * energy_tolerance + 1e-9, "{} 채널 {} 제곱합: {} / {}", name, channel_index, energy, expected_energy);
			}
		}
	}
}
