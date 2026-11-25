//==============================================================================
// 데스크탑 믹서. (cpal 의 소리 스레드에서 도는 WebAudio 노드 그래프, 원본이 쓰는 크롬 WebAudio 와 같은 셈)
// - 그래프: AudioBufferSourceNode → GainNode(플레이어) → [StereoPannerNode] → GainNode(버스) 또는 출력. 비프는 출력으로.
//   원본 AudioPlayer, AudioBus, SoundSystem.playEffect, BeepPlayer 가 짓는 그래프와 같다. (웹 백엔드 web.rs 와도 같다)
// - 128 프레임 렌더 퀀텀마다 그린다. (크롬 kRenderQuantumFrames) 명령은 퀀텀을 그리기 전에 받아, 값 바꾸기와 시작, 정지가 퀀텀 경계에서 걸린다.
// - 소스: 크롬 audio_buffer_source_handler.cc 를 옮겼다. 시작 위치(TimeToSampleFrame 의 1024 배 오버샘플 반올림), 재생 빠르기(버퍼 표본율 / 컨텍스트 표본율 x playbackRate),
//   빠르기 1 이고 위치가 정수면 그대로 베끼고(ProcessFastPath), 아니면 선형 보간(ProcessInterpolatedPath, 끝에서는 선형 외삽), 루프는 버퍼 전체를 되풀이한다.
// - GainNode: 값을 곱한다. (gain.value 대입은 크롬처럼 바로, 매끄럽게 하지 않는다)
// - StereoPannerNode: 크롬 stereo_panner.cc 의 PanToTargetValue. 모노는 x = (pan + 1) / 2 로 L = in cos(x π/2), R = in sin(x π/2),
//   스테레오는 pan <= 0 이면 L = L + R cos((pan + 1) π/2), R = R sin((pan + 1) π/2), 아니면 L = L cos(pan π/2), R = R + L sin(pan π/2). 이득은 f64 로 셈해 f32 로 곱한다.
// - 섞기: 모노를 스테레오 입력에 섞을 때는 스피커 규칙으로 양쪽에 같게 올린다. 출력에 내보낼 때 크롬처럼 -1 ~ 1 로 자른다.
// - 컨텍스트 멈춤(suspend) 동안은 그리지 않고 시계도 서며, 장치에는 0 을 낸다.
//==============================================================================
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;

// 렌더 퀀텀 프레임 수. (크롬 kRenderQuantumFrames)
pub(crate) const RENDER_QUANTUM_FRAMES: usize = 128;
// 재생 빠르기 한계. (크롬 kMaxRate)
const MAXIMUM_PLAYBACK_RATE: f64 = 1024.0;

//==============================================================================
// 푼 소리. (원본 AudioBuffer 자리, 1 채널이나 2 채널, decode 가 크롬처럼 푼 것)
//==============================================================================
pub struct AudioBufferData {
	pub(crate) sample_rate: f64,
	pub(crate) channel_list: Vec<Vec<f32>>,
}

impl AudioBufferData {
	pub(crate) fn get_length(&self) -> usize {
		self.channel_list.first().map(|channel| channel.len()).unwrap_or(0)
	}
}

//==============================================================================
// 소리 스레드에 보내는 명령.
//==============================================================================
pub(crate) enum MixerCommand {
	AddBus {
		bus_id: u64,
	},
	RemoveBus {
		bus_id: u64,
	},
	SetBusGain {
		bus_id: u64,
		gain: f32,
	},
	AddPlayer {
		player_id: u64,
		output_bus_id: Option<u64>,
		ended_generation: Arc<AtomicU64>,
	},
	RemovePlayer {
		player_id: u64,
	},
	SetPlayerGain {
		player_id: u64,
		gain: f32,
	},
	SetPlayerPan {
		player_id: u64,
		pan: f32,
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
	},
	PlayBeep {
		sample_list: Vec<f32>,
		delay_frame_count: u64,
	},
	Suspend,
	Resume,
}

//==============================================================================
// 소리 스레드와 나누는 값. (그린 프레임 수, 컨텍스트 시계)
//==============================================================================
pub(crate) struct MixerShared {
	pub(crate) rendered_frame_count: AtomicU64,
}

//==============================================================================
// 시간을 표본 자리로. (크롬 audio_utilities::TimeToSampleFrame, 1024 배로 셈해 반올림한 뒤 가장 가까운 정수)
//==============================================================================
fn time_to_sample_frame(time: f64, sample_rate: f64) -> f64 {
	const OVERSAMPLE_FACTOR: f64 = 1024.0;
	let frame = (time * sample_rate * OVERSAMPLE_FACTOR).round() / OVERSAMPLE_FACTOR;
	frame.round()
}

//==============================================================================
// AudioBufferSourceNode. (크롬 AudioBufferSourceHandler, 루프는 버퍼 전체, 빠르기는 0 보다 크다)
//==============================================================================
struct BufferSource {
	audio_buffer: Arc<AudioBufferData>,
	virtual_read_index: f64,
	computed_playback_rate: f64,
	is_looping: bool,
	is_finished: bool,
}

impl BufferSource {
	//==============================================================================
	// 생성. (start(0, offset): ClampGrainParameters 와 ComputePlaybackRate)
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
		}
	}

	//==============================================================================
	// 퀀텀 하나 그리기. (RenderFromBuffer, 채널 수를 돌려준다. 끝나면 is_finished)
	//==============================================================================
	fn render(&mut self, destination: &mut [[f32; RENDER_QUANTUM_FRAMES]; 2]) -> usize {
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
		let mut frames_to_process = RENDER_QUANTUM_FRAMES;
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
				channel[write_index..].fill(0.0);
			}
			self.is_finished = true;
		}
		self.virtual_read_index = virtual_read_index;
		channel_count
	}

	//==============================================================================
	// 그대로 베끼기. (ProcessFastPath)
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
	// 선형 보간. (ProcessInterpolatedPath, 빠르기가 0 보다 큰 쪽)
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
// 좌우 이득. (크롬 StereoPanner::PanToTargetValue, f64 로 셈해 f32 로)
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
// 플레이어. (원본 AudioPlayer 의 GainNode 와 지금 소스, 좌우 노드)
//==============================================================================
struct PlayerNode {
	player_id: u64,
	gain: f32,
	// StereoPannerNode 의 값. (0 이 아닌 좌우를 처음 걸 때 생기고 그 뒤로 남는다)
	pan: Option<f32>,
	output_bus_id: Option<u64>,
	source: Option<(BufferSource, u64)>,
	ended_generation: Arc<AtomicU64>,
}

//==============================================================================
// 버스. (원본 AudioBus 의 GainNode)
//==============================================================================
struct BusNode {
	bus_id: u64,
	gain: f32,
	input: [[f32; RENDER_QUANTUM_FRAMES]; 2],
}

//==============================================================================
// 비프. (오실레이터와 엔벨로프를 그린 모노 표본, 지연 뒤에 출력으로)
//==============================================================================
struct BeepVoice {
	sample_list: Vec<f32>,
	position: usize,
	delay_frame_count: u64,
}

//==============================================================================
// 소리 스레드의 믹서.
//==============================================================================
pub(crate) struct MixerRenderer {
	command_receiver: Receiver<MixerCommand>,
	shared: Arc<MixerShared>,
	sample_rate: f64,
	is_suspended: bool,
	player_list: Vec<PlayerNode>,
	bus_list: Vec<BusNode>,
	beep_list: Vec<BeepVoice>,
	// 그린 퀀텀. (출력 L, R 과 다음에 낼 자리)
	output_quantum: [[f32; RENDER_QUANTUM_FRAMES]; 2],
	output_position: usize,
	source_quantum: [[f32; RENDER_QUANTUM_FRAMES]; 2],
}

impl MixerRenderer {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub(crate) fn new(command_receiver: Receiver<MixerCommand>, shared: Arc<MixerShared>, sample_rate: f64) -> MixerRenderer {
		MixerRenderer {
			command_receiver,
			shared,
			sample_rate,
			is_suspended: false,
			player_list: Vec::with_capacity(256),
			bus_list: Vec::with_capacity(16),
			beep_list: Vec::with_capacity(16),
			output_quantum: [[0.0; RENDER_QUANTUM_FRAMES]; 2],
			output_position: RENDER_QUANTUM_FRAMES,
			source_quantum: [[0.0; RENDER_QUANTUM_FRAMES]; 2],
		}
	}

	//==============================================================================
	// 받은 명령 처리하기.
	//==============================================================================
	fn receive_commands(&mut self) {
		while let Ok(command) = self.command_receiver.try_recv() {
			match command {
				MixerCommand::AddBus { bus_id } => {
					self.bus_list.push(BusNode { bus_id, gain: 1.0, input: [[0.0; RENDER_QUANTUM_FRAMES]; 2] });
				}
				MixerCommand::RemoveBus { bus_id } => {
					self.bus_list.retain(|bus| bus.bus_id != bus_id);
				}
				MixerCommand::SetBusGain { bus_id, gain } => {
					if let Some(bus) = self.bus_list.iter_mut().find(|bus| bus.bus_id == bus_id) {
						bus.gain = gain;
					}
				}
				MixerCommand::AddPlayer { player_id, output_bus_id, ended_generation } => {
					self.player_list.push(PlayerNode { player_id, gain: 1.0, pan: None, output_bus_id, source: None, ended_generation });
				}
				MixerCommand::RemovePlayer { player_id } => {
					self.player_list.retain(|player| player.player_id != player_id);
				}
				MixerCommand::SetPlayerGain { player_id, gain } => {
					if let Some(player) = self.find_player(player_id) {
						player.gain = gain;
					}
				}
				MixerCommand::SetPlayerPan { player_id, pan } => {
					if let Some(player) = self.find_player(player_id) {
						if player.pan.is_some() || pan != 0.0 {
							player.pan = Some(pan);
						}
					}
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
				MixerCommand::StopSource { player_id } => {
					if let Some(player) = self.find_player(player_id) {
						if let Some((_, generation)) = player.source.take() {
							player.ended_generation.fetch_max(generation, Ordering::Release);
						}
					}
				}
				MixerCommand::PlayBeep { sample_list, delay_frame_count } => {
					self.beep_list.push(BeepVoice { sample_list, position: 0, delay_frame_count });
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

	fn find_player(&mut self, player_id: u64) -> Option<&mut PlayerNode> {
		self.player_list.iter_mut().find(|player| player.player_id == player_id)
	}

	//==============================================================================
	// 퀀텀 하나 그리기. (플레이어 → 버스 → 출력, 비프 → 출력)
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
		for player in self.player_list.iter_mut() {
			let Some((source, generation)) = player.source.as_mut() else {
				continue;
			};
			let channel_count = source.render(&mut self.source_quantum);
			if source.is_finished {
				player.ended_generation.fetch_max(*generation, Ordering::Release);
				player.source = None;
			}
			if channel_count == 0 {
				continue;
			}
			// GainNode.
			let gain = player.gain;
			for channel in self.source_quantum.iter_mut().take(channel_count) {
				for sample in channel.iter_mut() {
					*sample *= gain;
				}
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
			// 내보낼 곳에 섞기. (모노는 양쪽에 같게)
			let target = match player.output_bus_id {
				Some(bus_id) => {
					match self.bus_list.iter_mut().find(|bus| bus.bus_id == bus_id) {
						Some(bus) => {
							&mut bus.input
						}
						None => {
							continue;
						}
					}
				}
				None => {
					&mut self.output_quantum
				}
			};
			mix_into(target, &self.source_quantum, output_channel_count);
		}
		for bus in self.bus_list.iter() {
			let gain = bus.gain;
			for channel_index in 0..2 {
				for frame_index in 0..RENDER_QUANTUM_FRAMES {
					self.output_quantum[channel_index][frame_index] += bus.input[channel_index][frame_index] * gain;
				}
			}
		}
		for beep in self.beep_list.iter_mut() {
			for frame_index in 0..RENDER_QUANTUM_FRAMES {
				if beep.delay_frame_count > 0 {
					beep.delay_frame_count -= 1;
					continue;
				}
				let Some(sample) = beep.sample_list.get(beep.position) else {
					break;
				};
				self.output_quantum[0][frame_index] += *sample;
				self.output_quantum[1][frame_index] += *sample;
				beep.position += 1;
			}
		}
		self.beep_list.retain(|beep| beep.position < beep.sample_list.len());
		self.shared.rendered_frame_count.fetch_add(RENDER_QUANTUM_FRAMES as u64, Ordering::Release);
	}

	//==============================================================================
	// 장치 버퍼 채우기. (끼워 넣은 channel_count 채널, 앞 두 채널에 L, R 과 나머지 0, 모노 장치는 0.5 * (L + R). -1 ~ 1 로 자른다)
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
// 입력 섞기. (모노 입력은 스피커 규칙으로 양쪽에 같게 올린다)
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

	fn create_buffer(channel_list: Vec<Vec<f32>>, sample_rate: f64) -> Arc<AudioBufferData> {
		Arc::new(AudioBufferData { sample_rate, channel_list })
	}

	fn create_renderer(sample_rate: f64) -> (std::sync::mpsc::Sender<MixerCommand>, MixerRenderer, Arc<MixerShared>) {
		let (command_sender, command_receiver) = channel();
		let shared = Arc::new(MixerShared { rendered_frame_count: AtomicU64::new(0) });
		(command_sender, MixerRenderer::new(command_receiver, shared.clone(), sample_rate), shared)
	}

	//==============================================================================
	// 같은 표본율, 빠르기 1 은 그대로 베끼고 끝에서 멈춘다. (ProcessFastPath)
	//==============================================================================
	#[test]
	fn buffer_source_copies_and_finishes() {
		let buffer = create_buffer(vec![(0..200).map(|index| index as f32 / 200.0).collect()], 48000.0);
		let mut source = BufferSource::new(buffer, false, 0.0, 1.0, 48000.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		assert_eq!(source.render(&mut quantum), 1);
		assert_eq!(quantum[0][5], 5.0 / 200.0);
		assert!(!source.is_finished);
		source.render(&mut quantum);
		assert_eq!(quantum[0][71], 199.0 / 200.0);
		assert_eq!(quantum[0][72], 0.0);
		assert!(source.is_finished);
	}

	//==============================================================================
	// 표본율이 다르면 선형 보간하고, 끝 표본은 선형으로 넘겨 그린다. (ProcessInterpolatedPath)
	//==============================================================================
	#[test]
	fn buffer_source_interpolates_linearly() {
		let buffer = create_buffer(vec![vec![0.0, 1.0, 2.0, 3.0]], 44100.0);
		let mut source = BufferSource::new(buffer, false, 0.0, 1.0, 88200.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		source.render(&mut quantum);
		assert_eq!(&quantum[0][0..8], &[0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.5]);
		assert_eq!(quantum[0][8], 0.0);
		assert!(source.is_finished);
	}

	//==============================================================================
	// 루프는 버퍼 전체를 되풀이하고, 시작 위치는 1024 배 오버샘플로 반올림한다.
	//==============================================================================
	#[test]
	fn buffer_source_loops_from_offset() {
		let buffer = create_buffer(vec![vec![0.0, 1.0, 2.0, 3.0]], 44100.0);
		let mut source = BufferSource::new(buffer, true, 2.0 / 44100.0, 1.0, 44100.0);
		let mut quantum = [[0.0; RENDER_QUANTUM_FRAMES]; 2];
		source.render(&mut quantum);
		assert_eq!(&quantum[0][0..7], &[2.0, 3.0, 0.0, 1.0, 2.0, 3.0, 0.0]);
		assert!(!source.is_finished);
		assert_eq!(time_to_sample_frame(1.0 / 3.0, 44100.0), 14700.0);
	}

	//==============================================================================
	// 좌우: 모노는 가운데에서 양쪽 cos(π/4), 스테레오는 0 이면 그대로. (크롬 PanToTargetValue)
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
	// 그래프: 플레이어(음량, 좌우) → 버스(음량) → 출력. 모노는 양쪽에 올리고, 멈춘 동안은 그리지 않는다.
	//==============================================================================
	#[test]
	fn mixer_graph_applies_gain_pan_and_bus() {
		let (command_sender, mut renderer, shared) = create_renderer(44100.0);
		let ended_generation = Arc::new(AtomicU64::new(0));
		command_sender.send(MixerCommand::AddBus { bus_id: 1 }).unwrap();
		command_sender.send(MixerCommand::SetBusGain { bus_id: 1, gain: 0.5 }).unwrap();
		command_sender.send(MixerCommand::AddPlayer { player_id: 2, output_bus_id: Some(1), ended_generation: ended_generation.clone() }).unwrap();
		command_sender.send(MixerCommand::SetPlayerGain { player_id: 2, gain: 0.8 }).unwrap();
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
		let frame_count = shared.rendered_frame_count.load(Ordering::Acquire);
		command_sender.send(MixerCommand::Suspend).unwrap();
		let mut silent = vec![1.0f32; 2 * 300];
		renderer.render_interleaved(&mut silent, 2);
		assert!(silent.iter().all(|sample| *sample == 0.0));
		assert!(shared.rendered_frame_count.load(Ordering::Acquire) <= frame_count + RENDER_QUANTUM_FRAMES as u64);
	}

	//==============================================================================
	// 크롬(158) OfflineAudioContext 와 같은 그래프를 그려 견준다. (tests/assets/mixer/chromemixer.cjs 가 찍은 chrome_mixer.json)
	// - 소스(같은 표본율과 다른 표본율, 빠르기, 루프, 시작 위치), 플레이어 GainNode, StereoPannerNode(모노와 스테레오, 0 포함), 버스 GainNode, 섞기.
	//==============================================================================
	#[test]
	fn mixer_matches_chrome_offline_rendering() {
		let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/assets/mixer/chrome_mixer.json")).expect("chrome_mixer.json")).expect("json");
		let buffer_sample_rate = json["bufferSampleRate"].as_f64().unwrap();
		let buffer_length = json["bufferLength"].as_u64().unwrap() as usize;
		let render_length = json["renderLength"].as_u64().unwrap() as usize;
		let index_list: Vec<usize> = json["index"].as_array().unwrap().iter().map(|value| value.as_u64().unwrap() as usize).collect();
		let create_channel = |multiplier: usize, modulus: usize, offset: f64, divisor: f64| -> Vec<f32> {
			(0..buffer_length).map(|index| ((((index * multiplier) % modulus) as f64 - offset) / divisor) as f32).collect()
		};
		let stereo_buffer = create_buffer(vec![create_channel(37, 200, 100.0, 250.0), create_channel(53, 160, 80.0, 200.0)], buffer_sample_rate);
		let mono_buffer = create_buffer(vec![create_channel(29, 180, 90.0, 225.0)], buffer_sample_rate);
		let case_list = json["cases"].as_array().unwrap();
		assert_eq!(case_list.len(), 5);
		for test_case in case_list {
			let name = test_case["name"].as_str().unwrap();
			let (command_sender, mut renderer, _) = create_renderer(test_case["contextRate"].as_f64().unwrap());
			let mut next_id = 100u64;
			let mut bus_id_list: Vec<u64> = Vec::new();
			for bus_gain in test_case["busGainList"].as_array().unwrap() {
				next_id += 1;
				command_sender.send(MixerCommand::AddBus { bus_id: next_id }).unwrap();
				command_sender.send(MixerCommand::SetBusGain { bus_id: next_id, gain: bus_gain.as_f64().unwrap() as f32 }).unwrap();
				bus_id_list.push(next_id);
			}
			for player in test_case["playerList"].as_array().unwrap() {
				next_id += 1;
				let output_bus_id = player["bus"].as_u64().map(|bus_index| bus_id_list[bus_index as usize]);
				command_sender.send(MixerCommand::AddPlayer { player_id: next_id, output_bus_id, ended_generation: Arc::new(AtomicU64::new(0)) }).unwrap();
				command_sender.send(MixerCommand::SetPlayerGain { player_id: next_id, gain: player["gain"].as_f64().unwrap() as f32 }).unwrap();
				if let Some(pan) = player["pan"].as_f64() {
					// 원본처럼 0 이 아닌 좌우로 노드를 만든 뒤 값을 건다. (0 도 노드가 있는 꼴)
					command_sender.send(MixerCommand::SetPlayerPan { player_id: next_id, pan: 1.0 }).unwrap();
					command_sender.send(MixerCommand::SetPlayerPan { player_id: next_id, pan: pan as f32 }).unwrap();
				}
				let audio_buffer = if player["channels"].as_str().unwrap() == "stereo" { stereo_buffer.clone() } else { mono_buffer.clone() };
				command_sender.send(MixerCommand::StartSource { player_id: next_id, audio_buffer, is_loop: player["loop"].as_bool().unwrap(), offset_seconds: player["offset"].as_f64().unwrap(), playback_rate: player["rate"].as_f64().unwrap(), generation: 1 }).unwrap();
			}
			let mut output = vec![0.0f32; render_length * 2];
			renderer.render_interleaved(&mut output, 2);
			for channel_index in 0..2 {
				let value_list = test_case["values"][channel_index].as_array().unwrap();
				for (index, value) in index_list.iter().zip(value_list.iter()) {
					let expected = value.as_f64().unwrap();
					let actual = output[index * 2 + channel_index] as f64;
					assert!((actual - expected).abs() < 1e-6, "{} 채널 {} 자리 {}: {} / {}", name, channel_index, index, actual, expected);
				}
				let energy: f64 = (0..render_length).map(|index| (output[index * 2 + channel_index] as f64).powi(2)).sum();
				let expected_energy = test_case["energy"][channel_index].as_f64().unwrap();
				assert!((energy - expected_energy).abs() <= expected_energy * 1e-6 + 1e-9, "{} 채널 {} 제곱합: {} / {}", name, channel_index, energy, expected_energy);
			}
		}
	}
}
