//==============================================================================
//! 데스크탑 믹서 PannerNode 의 HRTF. (크로미엄 third_party/blink/renderer/platform/audio/hrtf_panner.cc, delay.cc 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 초안: 0.4.0 뒤 기능. (panner.rs 의 PannerNode 가 panningModel "HRTF" 일 때 쓴다)
//! - 자료와 커널은 hrtfdatabase.rs, FFT 와 합성곱은 fftframe.rs, 커널 보간이 쓰는 log10 과 pow 는 fdlibm.rs 에 있다. 자료와 고지는 resources/README.md.
//! - 셈(HRTFPanner::Pan): 방위각의 부호를 뒤집어(IRCAM 자료의 방위는 PannerNode 와 거꾸로) 0 ~ 360 을 192 칸으로 나눠 고르고, 칸 안 비는 지연만 섞는다.
//!   양쪽 귀를 따로 지연(Delay, k-rate 선형 보간)한 뒤 FFT 합성곱한다. 스테레오 입력은 왼쪽을 왼쪽 귀, 오른쪽을 오른쪽 귀로 한다.
//! - 방위 칸이나 높이가 바뀌면 두 합성곱 쌍 사이를 크로스페이드한다. (48000 Hz 이하 2048 프레임, 그 위 4096 프레임, 넘어가는 동안에는 새로 바꾸지 않는다)
//! - 퀀텀은 128 프레임 하나가 한 조각이다. 자동화가 있는 퀀텀(표본마다)도 크롬처럼 첫 표본의 방위와 높이로 셈한다.
//! - 꼬리는 지연 최대 2 ms 와 FFT 크기 / 2, 늦음은 FFT 크기 / 2 다. 입력이 조용해진 뒤 그만큼 더 셈한다. (PannerHandler::TailTime, LatencyTime)
//! - 데이터베이스는 표본율마다 따로 스레드에서 짓는다. 다 짓기 전에는 크롬 실시간 컨텍스트처럼 입력을 좌우 없이 그대로 낸다. (모노는 양쪽에 같게)
//==============================================================================
mod fdlibm;
mod fftframe;
mod hrtfdatabase;

pub(crate) use hrtfdatabase::HrtfDatabaseLoader;

use super::mixer::{time_to_sample_frame_rounded_up, RENDER_QUANTUM_FRAMES};
use fftframe::{multiply_add, FftConvolver};
use hrtfdatabase::{lerp_f64, NUMBER_OF_TOTAL_AZIMUTHS};
use std::sync::Arc;

type Quantum = [[f32; RENDER_QUANTUM_FRAMES]; 2];

/// 지연의 최대. (초, kMaxDelayTimeSeconds, 기본 데이터베이스의 가장 큰 커널 지연보다 크다)
const MAXIMUM_DELAY_TIME_SECONDS: f64 = 0.002;
/// 방위 칸을 아직 고르지 않음. (kUninitializedAzimuth)
const UNINITIALIZED_AZIMUTH: i32 = -1;

//==============================================================================
/// 표본율의 FFT 크기. (HRTFPanner::FftSizeForSampleRate, 응답 256 프레임을 그 표본율로 바꾼 길이 이하의 2 의 거듭제곱의 두 배)
///
/// # 인자
/// - `sample_rate`: 표본율.
///
/// # 돌려줌
/// 합성곱 FFT 크기. (44100, 48000 Hz 는 512)
//==============================================================================
pub(super) fn fft_size_for_sample_rate(sample_rate: f32) -> usize {
	const TRUNCATED_IMPULSE_LENGTH: f64 = 256.0;
	// sample_rate / 44100 은 크롬처럼 f32 셈이다.
	let sample_rate_ratio = (sample_rate / 44100.0f32) as f64;
	let resampled_length = TRUNCATED_IMPULSE_LENGTH * sample_rate_ratio;
	let analysis_fft_size = (1usize << (resampled_length.log2() as u32)).max(2);
	2 * analysis_fft_size
}

//==============================================================================
/// 방위각의 칸과 칸 안 비. (CalculateDesiredAzimuthIndexAndBlend, -180 ~ 180 을 0 ~ 360 으로 옮겨 192 칸으로 나눈다)
///
/// # 인자
/// - `azimuth`: 방위각. (도)
///
/// # 돌려줌
/// (칸 번호, 다음 칸 쪽 비).
//==============================================================================
fn calculate_desired_azimuth_index_and_blend(azimuth: f64) -> (i32, f64) {
	let azimuth = if azimuth < 0.0 { azimuth + 360.0 } else { azimuth };
	let number_of_azimuths = NUMBER_OF_TOTAL_AZIMUTHS as i32;
	let angle_between_azimuths = 360.0 / number_of_azimuths as f64;
	let desired_azimuth_index_float = azimuth / angle_between_azimuths;
	let desired_azimuth_index = desired_azimuth_index_float as i32;
	let azimuth_blend = desired_azimuth_index_float - desired_azimuth_index as f64;
	(desired_azimuth_index.clamp(0, number_of_azimuths - 1), azimuth_blend)
}

//==============================================================================
/// 지연. (Delay 의 k-rate 판, 원형 버퍼에 넣고 읽기 자리 사이를 선형 보간한다)
//==============================================================================
struct Delay {
	buffer: Vec<f32>,
	/// 지연 최대. (초, 크롬처럼 f32 로 든다)
	maximum_delay_time: f32,
	write_index: usize,
	desired_delay_frames: f64,
	temp_buffer: [f32; RENDER_QUANTUM_FRAMES],
	sample_rate: f32,
}

impl Delay {
	//==============================================================================
	/// 생성. (버퍼는 퀀텀 하나 + 최대 지연을 올림한 프레임)
	///
	/// # 인자
	/// - `maximum_delay_time`: 지연 최대. (초)
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 지연.
	//==============================================================================
	fn new(maximum_delay_time: f64, sample_rate: f32) -> Delay {
		let buffer_length = RENDER_QUANTUM_FRAMES + time_to_sample_frame_rounded_up(maximum_delay_time, sample_rate as f64) as usize;
		Delay {
			buffer: vec![0.0; buffer_length],
			maximum_delay_time: maximum_delay_time as f32,
			write_index: 0,
			desired_delay_frames: 0.0,
			temp_buffer: [0.0; RENDER_QUANTUM_FRAMES],
			sample_rate,
		}
	}

	//==============================================================================
	/// 지연 프레임 정하기. (SetDelayFrames)
	///
	/// # 인자
	/// - `frame_count`: 지연. (프레임)
	//==============================================================================
	fn set_delay_frames(&mut self, frame_count: f64) {
		self.desired_delay_frames = frame_count;
	}

	//==============================================================================
	/// 퀀텀 하나. (ProcessKRate: 지연을 0 ~ 최대로 자르고, 넣은 뒤 읽기 자리부터 꺼내 다음 표본과 선형 보간한다)
	///
	/// # 인자
	/// - `source`: 넣을 표본.
	/// - `destination`: 꺼낼 칸.
	//==============================================================================
	fn process_k_rate(&mut self, source: &[f32; RENDER_QUANTUM_FRAMES], destination: &mut [f32; RENDER_QUANTUM_FRAMES]) {
		let buffer_length = self.buffer.len();
		let frame_count = RENDER_QUANTUM_FRAMES;
		let sample_rate = self.sample_rate;
		let maximum_time = self.maximum_delay_time as f64;
		// DelayTime() 은 프레임 / 표본율이고, 다시 표본율을 곱해 프레임으로 돌린다.
		let mut delay_time = self.desired_delay_frames / sample_rate as f64;
		delay_time = super::pannermath::clamp_to(delay_time, 0.0, maximum_time);
		let desired_delay_frames = delay_time * sample_rate as f64;
		let mut write_index = self.write_index;
		let mut read_position = (write_index + buffer_length) as f64 - desired_delay_frames;
		if read_position >= buffer_length as f64 {
			read_position -= buffer_length as f64;
		}
		let read_index1 = read_position as usize;
		let interpolation_factor = (read_position - read_index1 as f64) as f32;
		// 원형 버퍼에 넣는다.
		let remainder = buffer_length - self.write_index;
		let first_size = frame_count.min(remainder);
		self.buffer[self.write_index..self.write_index + first_size].copy_from_slice(&source[..first_size]);
		if frame_count > remainder {
			self.buffer[..frame_count - remainder].copy_from_slice(&source[remainder..]);
		}
		write_index += frame_count;
		if write_index >= buffer_length {
			write_index -= buffer_length;
		}
		self.write_index = write_index;
		// 읽기 자리부터 꺼낸다.
		copy_from_circular_buffer(&self.buffer, read_index1, destination);
		if interpolation_factor != 0.0 {
			let read_index2 = (read_index1 + 1) % buffer_length;
			copy_from_circular_buffer(&self.buffer, read_index2, &mut self.temp_buffer);
			// dest[k] = dest[k] + f (temp[k] - dest[k]) (Vsub 뒤 Vsma)
			for (temp, output) in self.temp_buffer.iter_mut().zip(destination.iter_mut()) {
				*temp -= *output;
				*output = multiply_add(*temp, interpolation_factor, *output);
			}
		}
	}
}

//==============================================================================
/// 원형 버퍼에서 퀀텀 하나 꺼내기. (자리가 끝을 넘으면 앞에서 잇는다)
///
/// # 인자
/// - `buffer`: 원형 버퍼.
/// - `read_index`: 읽기 시작 자리.
/// - `destination`: 받을 칸.
//==============================================================================
fn copy_from_circular_buffer(buffer: &[f32], read_index: usize, destination: &mut [f32; RENDER_QUANTUM_FRAMES]) {
	let remainder = buffer.len() - read_index;
	let first_size = RENDER_QUANTUM_FRAMES.min(remainder);
	destination[..first_size].copy_from_slice(&buffer[read_index..read_index + first_size]);
	if RENDER_QUANTUM_FRAMES > remainder {
		destination[remainder..].copy_from_slice(&buffer[..RENDER_QUANTUM_FRAMES - remainder]);
	}
}

/// 두 합성곱 쌍 가운데 지금 쓰는 쪽. (CrossfadeSelection)
#[derive(Clone, Copy, PartialEq, Eq)]
enum CrossfadeSelection {
	First,
	Second,
}

//==============================================================================
/// HRTF 좌우. (HRTFPanner)
//==============================================================================
pub(crate) struct HrtfPanner {
	database_loader: Arc<HrtfDatabaseLoader>,
	sample_rate: f32,
	crossfade_selection: CrossfadeSelection,
	azimuth_index1: i32,
	elevation1: f64,
	azimuth_index2: i32,
	elevation2: f64,
	/// 크로스페이드 자리. (0 ~ 1)
	crossfade_x: f32,
	/// 표본마다 크로스페이드가 움직이는 양.
	crossfade_increment: f32,
	convolver_left1: FftConvolver,
	convolver_right1: FftConvolver,
	convolver_left2: FftConvolver,
	convolver_right2: FftConvolver,
	delay_line_left: Delay,
	delay_line_right: Delay,
	temp_left1: [f32; RENDER_QUANTUM_FRAMES],
	temp_right1: [f32; RENDER_QUANTUM_FRAMES],
	temp_left2: [f32; RENDER_QUANTUM_FRAMES],
	temp_right2: [f32; RENDER_QUANTUM_FRAMES],
}

impl HrtfPanner {
	//==============================================================================
	/// 생성. (HRTFPanner 생성자)
	///
	/// # 인자
	/// - `sample_rate`: 표본율.
	/// - `database_loader`: 그 표본율의 데이터베이스 로더.
	///
	/// # 돌려줌
	/// 좌우.
	//==============================================================================
	pub(crate) fn new(sample_rate: f32, database_loader: Arc<HrtfDatabaseLoader>) -> HrtfPanner {
		let fft_size = fft_size_for_sample_rate(sample_rate);
		HrtfPanner {
			database_loader,
			sample_rate,
			crossfade_selection: CrossfadeSelection::First,
			azimuth_index1: UNINITIALIZED_AZIMUTH,
			elevation1: 0.0,
			azimuth_index2: UNINITIALIZED_AZIMUTH,
			elevation2: 0.0,
			crossfade_x: 0.0,
			crossfade_increment: 0.0,
			convolver_left1: FftConvolver::new(fft_size),
			convolver_right1: FftConvolver::new(fft_size),
			convolver_left2: FftConvolver::new(fft_size),
			convolver_right2: FftConvolver::new(fft_size),
			delay_line_left: Delay::new(MAXIMUM_DELAY_TIME_SECONDS, sample_rate),
			delay_line_right: Delay::new(MAXIMUM_DELAY_TIME_SECONDS, sample_rate),
			temp_left1: [0.0; RENDER_QUANTUM_FRAMES],
			temp_right1: [0.0; RENDER_QUANTUM_FRAMES],
			temp_left2: [0.0; RENDER_QUANTUM_FRAMES],
			temp_right2: [0.0; RENDER_QUANTUM_FRAMES],
		}
	}

	//==============================================================================
	/// 표본율.
	///
	/// # 돌려줌
	/// 지은 표본율.
	//==============================================================================
	pub(crate) fn get_sample_rate(&self) -> f32 {
		self.sample_rate
	}

	//==============================================================================
	/// 꼬리 시간. (TailTime, 지연 최대 + FFT 크기 / 2 프레임)
	///
	/// # 돌려줌
	/// 초.
	//==============================================================================
	pub(crate) fn get_tail_time(&self) -> f64 {
		MAXIMUM_DELAY_TIME_SECONDS + (fft_size_for_sample_rate(self.sample_rate) / 2) as f64 / self.sample_rate as f64
	}

	//==============================================================================
	/// 늦음. (LatencyTime, FFT 크기 / 2 프레임)
	///
	/// # 돌려줌
	/// 초.
	//==============================================================================
	pub(crate) fn get_latency_time(&self) -> f64 {
		(fft_size_for_sample_rate(self.sample_rate) / 2) as f64 / self.sample_rate as f64
	}

	//==============================================================================
	/// 퀀텀 하나. (HRTFPanner::Pan, quantum 의 input_channel_count 채널을 받아 두 채널로 바꿔 쓴다)
	///
	/// # 인자
	/// - `desired_azimuth`: 방위각. (도, -180 ~ 180)
	/// - `elevation`: 높이. (도)
	/// - `quantum`: 입력이자 출력.
	/// - `input_channel_count`: 입력 채널 수. (1 이나 2)
	//==============================================================================
	pub(crate) fn pan(&mut self, desired_azimuth: f64, elevation: f64, quantum: &mut Quantum, input_channel_count: usize) {
		let database_loader = self.database_loader.clone();
		let Some(database) = database_loader.get_database()
		else {
			// 다 짓기 전에는 입력을 그대로 낸다. (CopyFrom, 스피커 규칙으로 모노는 양쪽에)
			if input_channel_count == 1 {
				quantum[1] = quantum[0];
			}
			return;
		};
		// IRCAM 자료의 방위는 PannerNode 와 거꾸로다.
		let azimuth = -desired_azimuth;
		let source_left = quantum[0];
		let source_right = if input_channel_count > 1 { quantum[1] } else { quantum[0] };
		let (desired_azimuth_index, azimuth_blend) = calculate_desired_azimuth_index_and_blend(azimuth);
		// 처음 만난 방위와 높이로 바로 맞춘다.
		if self.azimuth_index1 == UNINITIALIZED_AZIMUTH {
			self.azimuth_index1 = desired_azimuth_index;
			self.elevation1 = elevation;
		}
		if self.azimuth_index2 == UNINITIALIZED_AZIMUTH {
			self.azimuth_index2 = desired_azimuth_index;
			self.elevation2 = elevation;
		}
		// 크로스페이드는 45 ms 쯤. (경험값)
		let fade_frames = if self.sample_rate <= 48000.0 { 2048.0f64 } else { 4096.0f64 };
		// 방위나 높이가 바뀌었으면 크로스페이드를 시작한다.
		if self.crossfade_x == 0.0 && self.crossfade_selection == CrossfadeSelection::First && (desired_azimuth_index != self.azimuth_index1 || elevation != self.elevation1) {
			self.crossfade_increment = (1.0 / fade_frames) as f32;
			self.azimuth_index2 = desired_azimuth_index;
			self.elevation2 = elevation;
		}
		if self.crossfade_x == 1.0 && self.crossfade_selection == CrossfadeSelection::Second && (desired_azimuth_index != self.azimuth_index2 || elevation != self.elevation2) {
			self.crossfade_increment = (-1.0 / fade_frames) as f32;
			self.azimuth_index1 = desired_azimuth_index;
			self.elevation1 = elevation;
		}
		// 퀀텀 하나가 한 조각이다. (kFramesPerSegment = RenderQuantumFrames)
		let (kernel_left1, kernel_right1, frame_delay_left1, frame_delay_right1) = database.get_kernels_from_azimuth_elevation(azimuth_blend, self.azimuth_index1 as usize, self.elevation1);
		let (kernel_left2, kernel_right2, frame_delay_left2, frame_delay_right2) = database.get_kernels_from_azimuth_elevation(azimuth_blend, self.azimuth_index2 as usize, self.elevation2);
		// 귀 사이 지연도 크로스페이드한다.
		let frame_delay_left = lerp_f64(frame_delay_left1, frame_delay_left2, self.crossfade_x as f64);
		let frame_delay_right = lerp_f64(frame_delay_right1, frame_delay_right2, self.crossfade_x as f64);
		let [destination_left, destination_right] = quantum;
		// 먼저 귀 사이 시간차를 지연으로 건다.
		self.delay_line_left.set_delay_frames(frame_delay_left);
		self.delay_line_right.set_delay_frames(frame_delay_right);
		self.delay_line_left.process_k_rate(&source_left, destination_left);
		self.delay_line_right.process_k_rate(&source_right, destination_right);
		let needs_crossfading = self.crossfade_increment != 0.0;
		if needs_crossfading {
			// 두 쌍을 다 셈해 섞는다.
			self.temp_left1 = *destination_left;
			self.temp_right1 = *destination_right;
			self.temp_left2 = *destination_left;
			self.temp_right2 = *destination_right;
			self.convolver_left1.process(kernel_left1.get_fft_frame(), &mut self.temp_left1);
			self.convolver_right1.process(kernel_right1.get_fft_frame(), &mut self.temp_right1);
			self.convolver_left2.process(kernel_left2.get_fft_frame(), &mut self.temp_left2);
			self.convolver_right2.process(kernel_right2.get_fft_frame(), &mut self.temp_right2);
			let mut x = self.crossfade_x;
			let increment = self.crossfade_increment;
			for frame_index in 0..RENDER_QUANTUM_FRAMES {
				destination_left[frame_index] = (1.0 - x) * self.temp_left1[frame_index] + x * self.temp_left2[frame_index];
				destination_right[frame_index] = (1.0 - x) * self.temp_right1[frame_index] + x * self.temp_right2[frame_index];
				x += increment;
			}
			self.crossfade_x = x;
			if self.crossfade_increment > 0.0 && (self.crossfade_x - 1.0).abs() < self.crossfade_increment {
				// 1 에서 2 로 다 넘어갔다.
				self.crossfade_selection = CrossfadeSelection::Second;
				self.crossfade_x = 1.0;
				self.crossfade_increment = 0.0;
			}
			else if self.crossfade_increment < 0.0 && self.crossfade_x.abs() < -self.crossfade_increment {
				// 2 에서 1 로 다 넘어갔다.
				self.crossfade_selection = CrossfadeSelection::First;
				self.crossfade_x = 0.0;
				self.crossfade_increment = 0.0;
			}
		}
		else if self.crossfade_selection == CrossfadeSelection::First {
			self.convolver_left1.process(kernel_left1.get_fft_frame(), destination_left);
			self.convolver_right1.process(kernel_right1.get_fft_frame(), destination_right);
		}
		else {
			self.convolver_left2.process(kernel_left2.get_fft_frame(), destination_left);
			self.convolver_right2.process(kernel_right2.get_fft_frame(), destination_right);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// FFT 크기와 꼬리, 늦음. (44100, 48000 은 512, 96000 은 1024, 22050 은 256)
	//==============================================================================
	#[test]
	fn fft_size_follows_sample_rate() {
		assert_eq!(fft_size_for_sample_rate(44100.0), 512);
		assert_eq!(fft_size_for_sample_rate(48000.0), 512);
		assert_eq!(fft_size_for_sample_rate(96000.0), 1024);
		assert_eq!(fft_size_for_sample_rate(22050.0), 256);
		assert_eq!(fft_size_for_sample_rate(3000.0), 32);
		let (index, blend) = calculate_desired_azimuth_index_and_blend(-0.9375);
		assert_eq!(index, 191);
		assert!((blend - 0.5).abs() < 1e-9);
	}

	//==============================================================================
	/// 데이터베이스를 다 짓기 전에는 입력을 그대로 낸다. (크롬 실시간 컨텍스트, 모노는 양쪽에 같게, 스테레오는 그대로)
	//==============================================================================
	#[test]
	fn panner_passes_input_until_database_loaded() {
		let mut hrtf_panner = HrtfPanner::new(44100.0, HrtfDatabaseLoader::create_unloaded(44100.0));
		let mut quantum = [[0.0f32; RENDER_QUANTUM_FRAMES]; 2];
		for (index, sample) in quantum[0].iter_mut().enumerate() {
			*sample = index as f32 / 128.0;
		}
		let input = quantum[0];
		hrtf_panner.pan(30.0, 10.0, &mut quantum, 1);
		assert_eq!(quantum[0], input);
		assert_eq!(quantum[1], input);
		quantum[1] = [0.25; RENDER_QUANTUM_FRAMES];
		hrtf_panner.pan(-60.0, 0.0, &mut quantum, 2);
		assert_eq!(quantum[0], input);
		assert_eq!(quantum[1], [0.25; RENDER_QUANTUM_FRAMES]);
	}

	//==============================================================================
	/// 지연: 정수 지연은 그대로 늦추고, 반 프레임은 이웃 둘의 평균이다.
	//==============================================================================
	#[test]
	fn delay_shifts_and_interpolates() {
		let mut delay = Delay::new(MAXIMUM_DELAY_TIME_SECONDS, 44100.0);
		let mut source = [0.0f32; RENDER_QUANTUM_FRAMES];
		for (index, sample) in source.iter_mut().enumerate() {
			*sample = index as f32;
		}
		let mut destination = [0.0f32; RENDER_QUANTUM_FRAMES];
		delay.set_delay_frames(3.0);
		delay.process_k_rate(&source, &mut destination);
		assert_eq!(&destination[..5], &[0.0, 0.0, 0.0, 0.0, 1.0]);
		delay.set_delay_frames(2.5);
		delay.process_k_rate(&source, &mut destination);
		assert!((destination[10] - 7.5).abs() < 1e-5, "{}", destination[10]);
	}
}
