//==============================================================================
// OscillatorNode 와 PeriodicWave. (데스크탑 믹서의 비프. 크로미엄 oscillator_handler.cc, periodic_wave_handler.cc,
// cpu/arm/oscillator_kernel_neon.cc, cpu/x86/oscillator_kernel_sse2.cc, audio_scheduled_source_handler.cc 를 옮긴 것, Chromium Authors BSD-3-Clause)
// - 표(PeriodicWaveHandler::CreateBasicWaveform): 기본 파형의 사인 계수(GenerateBasicWaveform, f32 셈)로 대역 제한 표를 round(3 log2(표 크기)) 개 짓는다.
//   표 크기는 표본율 24000 이하 2048, 88200 이하 4096, 그 위 16384 칸이다. 표 r 은 배음을 (표 크기 / 2) x 2^(-r/3) 개까지 두고(CreateBandLimitedTables),
//   배음을 다 둔 첫 표의 가장 큰 크기의 역수를 모든 표에 곱한다. 크롬은 f32 역 FFT 로 짓고, 여기는 f64 역 FFT 로 지어 f32 로 내린다. (표 값 차이는 f32 끝자리)
// - 주파수로 두 표와 섞는 비를 고른다. (WaveDataForFundamentalFrequency, 1/3 옥타브마다 표 하나)
// - 퀀텀 처리(Process): 시작, 정지 프레임(UpdateSchedulingInfo, 올림), frequency AudioParam 이 자동화 중인 퀀텀은 표본마다 읽기 걸음을 셈하고(ProcessARate),
//   아니면 퀀텀에 하나다(ProcessKRate). 읽기 자리(virtual_read_index)와 보간은 크롬과 같은 차례, 같은 정밀도(f32, f64)로 셈한다.
//   걸음이 0.3 이상이면 두 점 선형 보간, 0.16 이상이면 세 점, 그 아래는 다섯 점 라그랑주 보간이다.
//   크롬처럼 표본마다의 걸음은 퀀텀 첫 표본부터 읽는다. (시작 퀀텀에서 시작 앞 표본 자리의 걸음을 쓴다)
// - 4 표본 묶음 셈은 크롬처럼 CPU 마다 다르다. aarch64 는 NEON 커널(묶음 읽기 자리를 f32 로 쌓는다), x86, x86_64 는 SSE2 커널, 그 밖은 크롬의 스칼라 셈이다.
// - detune 은 원본이 건드리지 않아 0 이다. (배율 2^(0/1200) = 1)
//==============================================================================
use super::audioparam::AudioParam;
use super::mixer::{time_to_sample_frame_rounded_up, RENDER_QUANTUM_FRAMES};
use std::sync::Arc;

// 한 옥타브를 나누는 표 수. (kNumberOfOctaveBands)
const NUMBER_OF_OCTAVE_BANDS: u32 = 3;
// 한 옥타브의 센트. (kCentsPerOctave)
const CENTS_PER_OCTAVE: f32 = 1200.0;
// 표 하나가 맡는 센트. (kCentsPerRange)
const CENTS_PER_RANGE: f32 = CENTS_PER_OCTAVE / NUMBER_OF_OCTAVE_BANDS as f32;
// 보간 점 수를 가르는 읽기 걸음. (kInterpolate2Point, kInterpolate3Point)
const INTERPOLATE_2_POINT: f32 = 0.3;
const INTERPOLATE_3_POINT: f32 = 0.16;
// frequency 의 기본값. (Hz)
const DEFAULT_FREQUENCY: f32 = 440.0;

//==============================================================================
// 파형. (원본 OscillatorNode.type)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OscillatorType {
	Sine,
	Square,
	Sawtooth,
	Triangle,
}

//==============================================================================
// 대역 제한 표 묶음. (크롬 PeriodicWaveHandler, 컨텍스트 표본율마다 파형 하나)
//==============================================================================
pub(crate) struct PeriodicWave {
	oscillator_type: OscillatorType,
	sample_rate: f32,
	periodic_wave_size: u32,
	number_of_ranges: u32,
	lowest_fundamental_frequency: f32,
	rate_scale: f32,
	band_limited_table_list: Vec<Vec<f32>>,
}

//==============================================================================
// 주파수 하나가 읽을 두 표와 섞는 비. (lower 는 배음을 덜 둔 표, higher 는 더 둔 표)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq)]
struct WaveData {
	lower_index: usize,
	higher_index: usize,
	table_interpolation_factor: f32,
}

//==============================================================================
// 표 크기. (GetPeriodicWaveSize)
//==============================================================================
fn get_periodic_wave_size(sample_rate: f32) -> u32 {
	if sample_rate <= 24000.0 {
		return 2048;
	}
	if sample_rate <= 88200.0 {
		return 4096;
	}
	16384
}

impl PeriodicWave {
	//==============================================================================
	// 기본 파형의 표 짓기. (CreateBasicWaveform, GenerateBasicWaveform)
	// - 사인 계수 b[n]: 사인 n = 1 만 1, 사각 홀수 4/(nπ), 톱니 ±2/(nπ), 삼각 홀수 ±8/(nπ)^2. 셈은 크롬처럼 f32 다.
	//==============================================================================
	pub(crate) fn create_basic_waveform(sample_rate: f32, oscillator_type: OscillatorType) -> PeriodicWave {
		let periodic_wave_size = get_periodic_wave_size(sample_rate);
		let half_size = (periodic_wave_size / 2) as usize;
		let mut coefficient_list = vec![0.0f32; half_size];
		for (harmonic_index, coefficient) in coefficient_list.iter_mut().enumerate().skip(1) {
			let is_odd = harmonic_index & 1 == 1;
			let pi_factor = 2.0 / (harmonic_index as f32 * std::f32::consts::PI);
			*coefficient = match oscillator_type {
				OscillatorType::Sine => {
					if harmonic_index == 1 {
						1.0
					}
					else {
						0.0
					}
				}
				OscillatorType::Square => {
					if is_odd {
						2.0 * pi_factor
					}
					else {
						0.0
					}
				}
				OscillatorType::Sawtooth => {
					pi_factor * if is_odd { 1.0 } else { -1.0 }
				}
				OscillatorType::Triangle => {
					if is_odd {
						2.0 * (pi_factor * pi_factor) * if ((harmonic_index - 1) >> 1) & 1 == 1 { -1.0 } else { 1.0 }
					}
					else {
						0.0
					}
				}
			};
		}
		let number_of_ranges = (NUMBER_OF_OCTAVE_BANDS as f32 * (periodic_wave_size as f32).log2()).round() as u32;
		let mut periodic_wave = PeriodicWave {
			oscillator_type,
			sample_rate,
			periodic_wave_size,
			number_of_ranges,
			lowest_fundamental_frequency: sample_rate / periodic_wave_size as f32,
			rate_scale: periodic_wave_size as f32 / sample_rate,
			band_limited_table_list: Vec::with_capacity(number_of_ranges as usize),
		};
		periodic_wave.create_band_limited_tables(&coefficient_list);
		periodic_wave
	}

	pub(crate) fn get_oscillator_type(&self) -> OscillatorType {
		self.oscillator_type
	}

	pub(crate) fn get_sample_rate(&self) -> f32 {
		self.sample_rate
	}

	//==============================================================================
	// 대역 제한 표들. (CreateBandLimitedTables, 정규화는 첫 표의 가장 큰 크기로)
	//==============================================================================
	fn create_band_limited_tables(&mut self, coefficient_list: &[f32]) {
		let periodic_wave_size = self.periodic_wave_size as usize;
		let half_size = periodic_wave_size / 2;
		let maximum_partial_count = half_size as u32;
		let component_count = coefficient_list.len().min(half_size);
		let sine_series = SineSeries::new(periodic_wave_size);
		let mut normalization_scale = 0.5f32;
		for range_index in 0..self.number_of_ranges {
			// 표 r 은 2^(-r/3) 배의 배음까지. (f32 배율을 내림)
			let cents_to_cull = range_index as f32 * CENTS_PER_RANGE;
			let culling_scale = 2.0f64.powf((-cents_to_cull / CENTS_PER_OCTAVE) as f64) as f32;
			let partial_count = (culling_scale * maximum_partial_count as f32) as u32;
			let kept_end = component_count.min(partial_count as usize + 1);
			let mut table = sine_series.synthesize(&coefficient_list[..kept_end]);
			if range_index == 0 {
				let maximum_value = table.iter().fold(0.0f32, |maximum_value, value| maximum_value.max(value.abs()));
				if maximum_value != 0.0 {
					normalization_scale = 1.0 / maximum_value;
				}
			}
			for value in table.iter_mut() {
				*value *= normalization_scale;
			}
			self.band_limited_table_list.push(table);
		}
	}

	//==============================================================================
	// 주파수의 두 표와 섞는 비. (WaveDataForFundamentalFrequency, 스칼라)
	//==============================================================================
	fn get_wave_data(&self, fundamental_frequency: f32) -> WaveData {
		let fundamental_frequency = fundamental_frequency.abs();
		let ratio = if fundamental_frequency > 0.0 {
			fundamental_frequency / self.lowest_fundamental_frequency
		}
		else {
			0.5
		};
		let cents_above_lowest_frequency = ratio.log2() * CENTS_PER_OCTAVE;
		let pitch_range = (1.0 + cents_above_lowest_frequency / CENTS_PER_RANGE).clamp(0.0, (self.number_of_ranges - 1) as f32);
		let range_index_1 = pitch_range as u32;
		let range_index_2 = if range_index_1 < self.number_of_ranges - 1 {
			range_index_1 + 1
		}
		else {
			range_index_1
		};
		WaveData {
			lower_index: range_index_2 as usize,
			higher_index: range_index_1 as usize,
			table_interpolation_factor: pitch_range - range_index_1 as f32,
		}
	}

	//==============================================================================
	// 주파수 넷의 두 표와 섞는 비. (NEON 판: 나누기 대신 역수 곱하기)
	//==============================================================================
	#[cfg(target_arch = "aarch64")]
	fn get_wave_data_vector(&self, frequency_list: [f32; 4]) -> [WaveData; 4] {
		let inverse_lowest_fundamental_frequency = 1.0 / self.lowest_fundamental_frequency;
		let maximum_range = (self.number_of_ranges - 1) as f32;
		frequency_list.map(|frequency| {
			let frequency = frequency.abs();
			let ratio = if frequency > 0.0 {
				frequency * inverse_lowest_fundamental_frequency
			}
			else {
				0.5
			};
			let cents_above_lowest_frequency = ratio.log2() * CENTS_PER_OCTAVE;
			let pitch_range = (1.0 + cents_above_lowest_frequency * (1.0 / CENTS_PER_RANGE)).max(0.0).min(maximum_range);
			let range_index_1 = pitch_range as u32;
			let range_index_2 = (range_index_1 + 1).min(self.number_of_ranges - 1);
			WaveData {
				lower_index: range_index_2 as usize,
				higher_index: range_index_1 as usize,
				table_interpolation_factor: pitch_range - range_index_1 as f32,
			}
		})
	}

	//==============================================================================
	// 주파수 넷의 두 표와 섞는 비. (SSE2 판은 스칼라와 같은 셈, 그 밖은 스칼라)
	//==============================================================================
	#[cfg(not(target_arch = "aarch64"))]
	fn get_wave_data_vector(&self, frequency_list: [f32; 4]) -> [WaveData; 4] {
		frequency_list.map(|frequency| self.get_wave_data(frequency))
	}

	fn get_table_pair(&self, wave_data: &WaveData) -> (&[f32], &[f32]) {
		(&self.band_limited_table_list[wave_data.lower_index], &self.band_limited_table_list[wave_data.higher_index])
	}
}

//==============================================================================
// 사인 급수 합. (f64 역 FFT 로 표 칸마다 Σ b[k] sin(2πkn/N), 크롬 FFTFrame::DoInverseFFT 자리)
//==============================================================================
struct SineSeries {
	size: usize,
	// e^(+i 2πj/N) 의 (cos, sin). (j = 0 ~ N/2 - 1)
	twiddle_list: Vec<(f64, f64)>,
	bit_reversed_index_list: Vec<usize>,
}

impl SineSeries {
	fn new(size: usize) -> SineSeries {
		let bit_count = size.trailing_zeros();
		SineSeries {
			size,
			twiddle_list: (0..size / 2).map(|index| {
				let angle = std::f64::consts::TAU * index as f64 / size as f64;
				(angle.cos(), angle.sin())
			}).collect(),
			bit_reversed_index_list: (0..size).map(|index| index.reverse_bits() >> (usize::BITS - bit_count)).collect(),
		}
	}

	//==============================================================================
	// 표 하나. (coefficient_list[k] 가 k 번 배음의 사인 계수, 0 번은 쓰지 않는다)
	//==============================================================================
	fn synthesize(&self, coefficient_list: &[f32]) -> Vec<f32> {
		let size = self.size;
		let mut real_list = vec![0.0f64; size];
		let mut imaginary_list = vec![0.0f64; size];
		for (harmonic_index, coefficient) in coefficient_list.iter().enumerate().skip(1) {
			real_list[self.bit_reversed_index_list[harmonic_index]] = *coefficient as f64;
		}
		let mut length = 2;
		while length <= size {
			let half_length = length / 2;
			let twiddle_step = size / length;
			for start_index in (0..size).step_by(length) {
				for offset in 0..half_length {
					let (twiddle_real, twiddle_imaginary) = self.twiddle_list[offset * twiddle_step];
					let first_index = start_index + offset;
					let second_index = first_index + half_length;
					let product_real = real_list[second_index] * twiddle_real - imaginary_list[second_index] * twiddle_imaginary;
					let product_imaginary = real_list[second_index] * twiddle_imaginary + imaginary_list[second_index] * twiddle_real;
					real_list[second_index] = real_list[first_index] - product_real;
					imaginary_list[second_index] = imaginary_list[first_index] - product_imaginary;
					real_list[first_index] += product_real;
					imaginary_list[first_index] += product_imaginary;
				}
			}
			length *= 2;
		}
		imaginary_list.iter().map(|value| *value as f32).collect()
	}
}

//==============================================================================
// 재생 상태. (AudioScheduledSourceHandler, start 를 부른 뒤부터)
//==============================================================================
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlaybackState {
	Scheduled,
	Playing,
	Finished,
}

//==============================================================================
// OscillatorNode. (크롬 OscillatorHandler, 출력은 모노)
//==============================================================================
pub(crate) struct OscillatorNode {
	periodic_wave: Arc<PeriodicWave>,
	// 주파수. (Hz, a-rate, 범위는 ± 나이퀴스트)
	pub(crate) frequency: AudioParam,
	start_time: f64,
	end_time: Option<f64>,
	playback_state: PlaybackState,
	// 표 읽기 자리. (표 칸 단위)
	virtual_read_index: f64,
	phase_increment_list: [f32; RENDER_QUANTUM_FRAMES],
}

impl OscillatorNode {
	//==============================================================================
	// 생성하고 시작, 정지 예약. (start_time 은 부른 때의 시계보다 앞이 아니다, 크롬 Start 의 max(when, currentTime))
	//==============================================================================
	pub(crate) fn new(periodic_wave: Arc<PeriodicWave>, sample_rate: f64, start_time: f64, end_time: f64) -> OscillatorNode {
		let nyquist = sample_rate as f32 / 2.0;
		OscillatorNode {
			periodic_wave,
			frequency: AudioParam::new(DEFAULT_FREQUENCY, -nyquist, nyquist),
			start_time,
			end_time: Some(end_time.max(0.0)),
			playback_state: PlaybackState::Scheduled,
			virtual_read_index: 0.0,
			phase_increment_list: [0.0; RENDER_QUANTUM_FRAMES],
		}
	}

	pub(crate) fn is_finished(&self) -> bool {
		self.playback_state == PlaybackState::Finished
	}

	//==============================================================================
	// 퀀텀 하나 그리기. (Process, 소리를 낸 표본이 있으면 참)
	//==============================================================================
	pub(crate) fn process(&mut self, output: &mut [f32; RENDER_QUANTUM_FRAMES], current_frame: u64, sample_rate: f64) -> bool {
		output.fill(0.0);
		let (quantum_frame_offset, non_silent_frame_count, start_frame_offset) = self.update_scheduling_info(output, current_frame, sample_rate);
		if non_silent_frame_count == 0 {
			return false;
		}
		let nyquist = sample_rate as f32 / 2.0;
		let rate_scale = self.periodic_wave.rate_scale;
		let has_sample_accurate_values = self.calculate_sample_accurate_phase_increments(current_frame, sample_rate);
		let mut frequency = 0.0f32;
		if !has_sample_accurate_values {
			frequency = clamp_frequency(self.frequency.calculate_quantum_value(current_frame, sample_rate, RENDER_QUANTUM_FRAMES), nyquist);
		}
		let mut virtual_read_index = self.virtual_read_index;
		let mut destination_index = quantum_frame_offset;
		let mut frame_count = non_silent_frame_count;
		// 표본 사이에서 시작하면 첫 표본은 0 이고 그만큼 읽기 자리를 옮긴다. (a-rate 퀀텀은 frequency 가 0 이라 옮기지 않는다)
		if start_frame_offset > 0.0 {
			destination_index += 1;
			frame_count -= 1;
			virtual_read_index += (1.0 - start_frame_offset) * frequency as f64 * rate_scale as f64;
		}
		else if start_frame_offset < 0.0 {
			virtual_read_index = -start_frame_offset * frequency as f64 * rate_scale as f64;
		}
		let destination = &mut output[destination_index..destination_index + frame_count];
		self.virtual_read_index = if has_sample_accurate_values {
			process_a_rate(&self.periodic_wave, destination, virtual_read_index, &self.phase_increment_list)
		}
		else {
			process_k_rate(&self.periodic_wave, destination, virtual_read_index, frequency)
		};
		true
	}

	//==============================================================================
	// 시작, 정지 프레임. (UpdateSchedulingInfo: (퀀텀 안 시작 자리, 그릴 표본 수, 시작의 표본 사이 어긋남))
	//==============================================================================
	fn update_scheduling_info(&mut self, output: &mut [f32; RENDER_QUANTUM_FRAMES], current_frame: u64, sample_rate: f64) -> (usize, usize, f64) {
		let quantum_start_frame = current_frame;
		let quantum_end_frame = current_frame + RENDER_QUANTUM_FRAMES as u64;
		let start_frame = time_to_sample_frame_rounded_up(self.start_time, sample_rate);
		let end_frame = self.end_time.map(|end_time| time_to_sample_frame_rounded_up(end_time, sample_rate));
		if end_frame.map(|end_frame| end_frame <= quantum_start_frame).unwrap_or(false) {
			self.playback_state = PlaybackState::Finished;
		}
		if self.playback_state == PlaybackState::Finished || start_frame >= quantum_end_frame {
			return (0, 0, 0.0);
		}
		let mut start_frame_offset = 0.0;
		if self.playback_state == PlaybackState::Scheduled {
			self.playback_state = PlaybackState::Playing;
			start_frame_offset = self.start_time * sample_rate - start_frame as f64;
		}
		let quantum_frame_offset = (start_frame.saturating_sub(quantum_start_frame) as usize).min(RENDER_QUANTUM_FRAMES);
		let mut non_silent_frame_count = RENDER_QUANTUM_FRAMES - quantum_frame_offset;
		if non_silent_frame_count == 0 {
			return (quantum_frame_offset, 0, start_frame_offset);
		}
		if let Some(end_frame) = end_frame {
			if end_frame >= quantum_start_frame && end_frame <= quantum_end_frame {
				if end_frame < quantum_end_frame {
					let zero_start_frame = (end_frame - quantum_start_frame) as usize;
					let zero_frame_count = RENDER_QUANTUM_FRAMES - zero_start_frame;
					non_silent_frame_count = non_silent_frame_count.saturating_sub(zero_frame_count);
					output[zero_start_frame..].fill(0.0);
				}
				self.playback_state = PlaybackState::Finished;
			}
		}
		(quantum_frame_offset, non_silent_frame_count, start_frame_offset)
	}

	//==============================================================================
	// 표본마다의 읽기 걸음. (CalculateSampleAccuratePhaseIncrements: frequency 가 자동화 중이면 퀀텀 표본마다 주파수 x 표 크기 / 표본율)
	//==============================================================================
	fn calculate_sample_accurate_phase_increments(&mut self, current_frame: u64, sample_rate: f64) -> bool {
		if !self.frequency.has_sample_accurate_values(current_frame, sample_rate, RENDER_QUANTUM_FRAMES) {
			return false;
		}
		self.frequency.calculate_sample_accurate_values(&mut self.phase_increment_list, current_frame, sample_rate);
		let nyquist = sample_rate as f32 / 2.0;
		let final_scale = self.periodic_wave.rate_scale;
		for phase_increment in self.phase_increment_list.iter_mut() {
			*phase_increment = clamp_frequency(*phase_increment, nyquist) * final_scale;
		}
		true
	}
}

//==============================================================================
// 주파수를 ± 나이퀴스트로. (ClampFrequency, NaN 은 나이퀴스트)
//==============================================================================
fn clamp_frequency(frequency: f32, nyquist: f32) -> f32 {
	if frequency.is_nan() {
		return nyquist;
	}
	frequency.clamp(-nyquist, nyquist)
}

//==============================================================================
// 읽기 자리를 0 ~ 표 크기로. (f64)
//==============================================================================
fn wrap_virtual_index(virtual_index: f64, periodic_wave_size: u32) -> f64 {
	let periodic_wave_size = periodic_wave_size as f64;
	virtual_index - (virtual_index * (1.0 / periodic_wave_size)).floor() * periodic_wave_size
}

//==============================================================================
// 읽기 자리를 0 ~ 표 크기로. (f32, 크롬 WrapVirtualIndexVector: 0 쪽으로 자른 뒤 넘쳤으면 하나 빼기)
//==============================================================================
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
fn wrap_virtual_index_f32(virtual_index: f32, wave_size: f32, inverse_wave_size: f32) -> f32 {
	let ratio = virtual_index * inverse_wave_size;
	let mut floor_value = ratio as i32;
	if ratio < floor_value as f32 {
		floor_value -= 1;
	}
	virtual_index - floor_value as f32 * wave_size
}

//==============================================================================
// 보간. (DoInterpolation: 걸음 0.3 이상 두 점 선형, 0.16 이상 세 점, 그 아래 다섯 점 라그랑주. f64 로 셈한다)
//==============================================================================
fn do_interpolation(periodic_wave: &PeriodicWave, virtual_read_index: f64, increment: f32, read_index_mask: u32, wave_data: &WaveData) -> f32 {
	let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(wave_data);
	let mut sample_lower = 0.0f64;
	let mut sample_higher = 0.0f64;
	let read_index_0 = virtual_read_index as u32;
	if increment >= INTERPOLATE_2_POINT {
		let interpolation_factor = ((virtual_read_index as f32) - read_index_0 as f32) as f64;
		let read_index_2 = (read_index_0.wrapping_add(1) & read_index_mask) as usize;
		let read_index_1 = (read_index_0 & read_index_mask) as usize;
		sample_higher = (1.0 - interpolation_factor) * higher_wave_data[read_index_1] as f64 + interpolation_factor * higher_wave_data[read_index_2] as f64;
		sample_lower = (1.0 - interpolation_factor) * lower_wave_data[read_index_1] as f64 + interpolation_factor * lower_wave_data[read_index_2] as f64;
	}
	else if increment >= INTERPOLATE_3_POINT {
		let t = virtual_read_index - read_index_0 as f64;
		let weight_list = [0.5 * t * (t - 1.0), 1.0 - t * t, 0.5 * t * (t + 1.0)];
		for (offset, weight) in weight_list.iter().enumerate() {
			let read_index = (read_index_0.wrapping_add(offset as u32).wrapping_sub(1) & read_index_mask) as usize;
			sample_lower += weight * lower_wave_data[read_index] as f64;
			sample_higher += weight * higher_wave_data[read_index] as f64;
		}
	}
	else {
		let t = virtual_read_index - read_index_0 as f64;
		let t2 = t * t;
		let weight_list = [
			t * (t2 - 1.0) * (t - 2.0) / 24.0,
			-t * (t - 1.0) * (t2 - 4.0) / 6.0,
			(t2 - 1.0) * (t2 - 4.0) / 4.0,
			-t * (t + 1.0) * (t2 - 4.0) / 6.0,
			t * (t2 - 1.0) * (t + 2.0) / 24.0,
		];
		for (offset, weight) in weight_list.iter().enumerate() {
			let read_index = (read_index_0.wrapping_add(offset as u32).wrapping_sub(2) & read_index_mask) as usize;
			sample_lower += weight * lower_wave_data[read_index] as f64;
			sample_higher += weight * higher_wave_data[read_index] as f64;
		}
	}
	let table_interpolation_factor = wave_data.table_interpolation_factor;
	((1.0 - table_interpolation_factor) as f64 * sample_higher + table_interpolation_factor as f64 * sample_lower) as f32
}

//==============================================================================
// 두 점 선형 보간 하나. (f32, x0 + f (x1 - x0) 로 표 안, 표 사이)
//==============================================================================
fn interpolate_linear(lower_wave_data: &[f32], higher_wave_data: &[f32], read_index_0: usize, read_index_1: usize, interpolation_factor: f32, table_interpolation_factor: f32) -> f32 {
	let sample_higher = higher_wave_data[read_index_0] + interpolation_factor * (higher_wave_data[read_index_1] - higher_wave_data[read_index_0]);
	let sample_lower = lower_wave_data[read_index_0] + interpolation_factor * (lower_wave_data[read_index_1] - lower_wave_data[read_index_0]);
	sample_higher + table_interpolation_factor * (sample_lower - sample_higher)
}

//==============================================================================
// 표본마다 걸음이 다른 퀀텀. (ProcessARate: 4 표본 묶음 뒤 남은 표본은 스칼라)
//==============================================================================
fn process_a_rate(periodic_wave: &PeriodicWave, destination: &mut [f32], virtual_read_index: f64, phase_increment_list: &[f32]) -> f64 {
	let (processed_frame_count, virtual_read_index) = process_a_rate_vector(periodic_wave, destination, virtual_read_index, phase_increment_list);
	process_a_rate_scalar(periodic_wave, processed_frame_count, destination, virtual_read_index, phase_increment_list)
}

//==============================================================================
// 4 표본 묶음. (ProcessARateVector: 넷이 모두 걸음 0.3 이상이면 커널, 아니면 표본마다 DoInterpolation)
//==============================================================================
fn process_a_rate_vector(periodic_wave: &PeriodicWave, destination: &mut [f32], mut virtual_read_index: f64, phase_increment_list: &[f32]) -> (usize, f64) {
	let inverse_rate_scale = 1.0 / periodic_wave.rate_scale;
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let mut frame_index = 0;
	for _ in 0..destination.len() / 4 {
		let mut is_big_increment = true;
		let mut frequency_list = [0.0f32; 4];
		for (offset, frequency) in frequency_list.iter_mut().enumerate() {
			let phase_increment = phase_increment_list[frame_index + offset];
			is_big_increment = is_big_increment && phase_increment.abs() >= INTERPOLATE_2_POINT;
			*frequency = inverse_rate_scale * phase_increment;
		}
		let wave_data_list = periodic_wave.get_wave_data_vector(frequency_list);
		if is_big_increment {
			virtual_read_index = process_a_rate_vector_kernel(periodic_wave, &mut destination[frame_index..frame_index + 4], virtual_read_index, &phase_increment_list[frame_index..frame_index + 4], &wave_data_list);
		}
		else {
			for (offset, wave_data) in wave_data_list.iter().enumerate() {
				let phase_increment = phase_increment_list[frame_index + offset];
				destination[frame_index + offset] = do_interpolation(periodic_wave, virtual_read_index, phase_increment.abs(), read_index_mask, wave_data);
				virtual_read_index = wrap_virtual_index(virtual_read_index + phase_increment as f64, periodic_wave_size);
			}
		}
		frame_index += 4;
	}
	(frame_index, virtual_read_index)
}

//==============================================================================
// 남은 표본. (ProcessARateScalar)
//==============================================================================
fn process_a_rate_scalar(periodic_wave: &PeriodicWave, start_index: usize, destination: &mut [f32], mut virtual_read_index: f64, phase_increment_list: &[f32]) -> f64 {
	let inverse_rate_scale = 1.0 / periodic_wave.rate_scale;
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	for frame_index in start_index..destination.len() {
		let phase_increment = phase_increment_list[frame_index];
		let wave_data = periodic_wave.get_wave_data(inverse_rate_scale * phase_increment);
		destination[frame_index] = do_interpolation(periodic_wave, virtual_read_index, phase_increment.abs(), read_index_mask, &wave_data);
		virtual_read_index = wrap_virtual_index(virtual_read_index + phase_increment as f64, periodic_wave_size);
	}
	virtual_read_index
}

//==============================================================================
// a-rate 4 표본 커널. (NEON 판: 걸음 합은 f64, 읽기 자리는 f32 로 내려 다시 감싼다)
//==============================================================================
#[cfg(target_arch = "aarch64")]
fn process_a_rate_vector_kernel(periodic_wave: &PeriodicWave, destination: &mut [f32], virtual_read_index: f64, phase_increment_list: &[f32], wave_data_list: &[WaveData; 4]) -> f64 {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let increment_sum_list = get_increment_sum_list(phase_increment_list);
	let virtual_index_list = [
		wrap_virtual_index(virtual_read_index, periodic_wave_size),
		wrap_virtual_index(virtual_read_index + increment_sum_list[0], periodic_wave_size),
		wrap_virtual_index(virtual_read_index + increment_sum_list[1], periodic_wave_size),
		wrap_virtual_index(virtual_read_index + increment_sum_list[2], periodic_wave_size),
	];
	let wave_size = periodic_wave_size as f32;
	let inverse_wave_size = (1.0 / periodic_wave_size as f64) as f32;
	for (offset, wave_data) in wave_data_list.iter().enumerate() {
		let virtual_index = wrap_virtual_index_f32(virtual_index_list[offset] as f32, wave_size, inverse_wave_size);
		let read_index_0 = (virtual_index as u32) & read_index_mask;
		let read_index_1 = (read_index_0 + 1) & read_index_mask;
		let interpolation_factor = virtual_index - read_index_0 as f32;
		let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(wave_data);
		destination[offset] = interpolate_linear(lower_wave_data, higher_wave_data, read_index_0 as usize, read_index_1 as usize, interpolation_factor, wave_data.table_interpolation_factor);
	}
	wrap_virtual_index(virtual_read_index + increment_sum_list[3], periodic_wave_size)
}

//==============================================================================
// a-rate 4 표본 커널. (SSE2 판: 읽기 자리는 f64 로 감싸고 자른 뒤, 보간 비만 f32)
//==============================================================================
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn process_a_rate_vector_kernel(periodic_wave: &PeriodicWave, destination: &mut [f32], virtual_read_index: f64, phase_increment_list: &[f32], wave_data_list: &[WaveData; 4]) -> f64 {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let increment_sum_list = get_increment_sum_list(phase_increment_list);
	let wave_size = periodic_wave_size as f64;
	let inverse_wave_size = 1.0 / wave_size;
	let virtual_index_list = [virtual_read_index, virtual_read_index + increment_sum_list[0], virtual_read_index + increment_sum_list[1], virtual_read_index + increment_sum_list[2]].map(|virtual_index| {
		let ratio = virtual_index * inverse_wave_size;
		let mut floor_value = ratio as i32;
		if ratio < floor_value as f64 {
			floor_value -= 1;
		}
		virtual_index - floor_value as f64 * wave_size
	});
	for (offset, wave_data) in wave_data_list.iter().enumerate() {
		let virtual_index = virtual_index_list[offset];
		let read_index = virtual_index as i32;
		let interpolation_factor = virtual_index as f32 - read_index as f32;
		let read_index_0 = (read_index as u32) & read_index_mask;
		let read_index_1 = ((read_index + 1) as u32) & read_index_mask;
		let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(wave_data);
		destination[offset] = interpolate_linear(lower_wave_data, higher_wave_data, read_index_0 as usize, read_index_1 as usize, interpolation_factor, wave_data.table_interpolation_factor);
	}
	wrap_virtual_index(virtual_read_index + increment_sum_list[3], periodic_wave_size)
}

//==============================================================================
// a-rate 4 표본 커널. (벡터가 없는 CPU 의 크롬 스칼라 판: 표 안 보간 비는 f64)
//==============================================================================
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
fn process_a_rate_vector_kernel(periodic_wave: &PeriodicWave, destination: &mut [f32], mut virtual_read_index: f64, phase_increment_list: &[f32], wave_data_list: &[WaveData; 4]) -> f64 {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	for (offset, wave_data) in wave_data_list.iter().enumerate() {
		let read_index = virtual_read_index as u32;
		let interpolation_factor = ((virtual_read_index as f32) - read_index as f32) as f64;
		let read_index_0 = (read_index & read_index_mask) as usize;
		let read_index_1 = (read_index.wrapping_add(1) & read_index_mask) as usize;
		let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(wave_data);
		let sample_higher = (higher_wave_data[read_index_0] as f64 + interpolation_factor * (higher_wave_data[read_index_1] - higher_wave_data[read_index_0]) as f64) as f32;
		let sample_lower = (lower_wave_data[read_index_0] as f64 + interpolation_factor * (lower_wave_data[read_index_1] - lower_wave_data[read_index_0]) as f64) as f32;
		destination[offset] = sample_higher + wave_data.table_interpolation_factor * (sample_lower - sample_higher);
		virtual_read_index = wrap_virtual_index(virtual_read_index + phase_increment_list[offset] as f64, periodic_wave_size);
	}
	virtual_read_index
}

//==============================================================================
// 걸음 누적 합. (f64)
//==============================================================================
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
fn get_increment_sum_list(phase_increment_list: &[f32]) -> [f64; 4] {
	let mut increment_sum_list = [0.0f64; 4];
	increment_sum_list[0] = phase_increment_list[0] as f64;
	for offset in 1..4 {
		increment_sum_list[offset] = increment_sum_list[offset - 1] + phase_increment_list[offset] as f64;
	}
	increment_sum_list
}

//==============================================================================
// 퀀텀에 걸음 하나. (ProcessKRate: 걸음 0.3 이상이면 4 표본 묶음과 남은 표본, 읽기 자리는 처음 + n x 걸음. 아니면 표본마다 DoInterpolation)
//==============================================================================
fn process_k_rate(periodic_wave: &PeriodicWave, destination: &mut [f32], mut virtual_read_index: f64, frequency: f32) -> f64 {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let wave_data = periodic_wave.get_wave_data(frequency);
	let rate_scale = periodic_wave.rate_scale;
	let increment = frequency * rate_scale;
	let frame_count = destination.len();
	if increment >= INTERPOLATE_2_POINT {
		let (processed_frame_count, vector_read_index) = process_k_rate_vector(periodic_wave, destination, virtual_read_index, frequency, rate_scale);
		if processed_frame_count < frame_count {
			process_k_rate_scalar(periodic_wave, processed_frame_count, destination, vector_read_index, frequency, rate_scale);
		}
		virtual_read_index += (frame_count as f32 * increment) as f64;
		return wrap_virtual_index(virtual_read_index, periodic_wave_size);
	}
	for sample in destination.iter_mut() {
		*sample = do_interpolation(periodic_wave, virtual_read_index, increment.abs(), read_index_mask, &wave_data);
		virtual_read_index = wrap_virtual_index(virtual_read_index + increment as f64, periodic_wave_size);
	}
	virtual_read_index
}

//==============================================================================
// k-rate 남은 표본. (ProcessKRateScalar: 표 안 보간 비는 f32)
//==============================================================================
fn process_k_rate_scalar(periodic_wave: &PeriodicWave, start_index: usize, destination: &mut [f32], mut virtual_read_index: f64, frequency: f32, rate_scale: f32) -> f64 {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let wave_data = periodic_wave.get_wave_data(frequency);
	let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(&wave_data);
	let increment = frequency * rate_scale;
	for sample in destination.iter_mut().skip(start_index) {
		let read_index = virtual_read_index as u32;
		let interpolation_factor = (virtual_read_index as f32) - read_index as f32;
		let read_index_0 = read_index & read_index_mask;
		let read_index_1 = (read_index_0 + 1) & read_index_mask;
		*sample = interpolate_linear(lower_wave_data, higher_wave_data, read_index_0 as usize, read_index_1 as usize, interpolation_factor, wave_data.table_interpolation_factor);
		virtual_read_index = wrap_virtual_index(virtual_read_index + increment as f64, periodic_wave_size);
	}
	virtual_read_index
}

//==============================================================================
// k-rate 4 표본 묶음. (NEON 판은 처음 읽기 자리를 f32 로 내린 뒤 걸음을 f32 로 더하고, SSE2 판은 f64 로 더한 뒤 f32 로 내린다.
// 묶음마다 4 x 걸음을 f32 로 더해 감싼다. 돌려주는 읽기 자리는 처음 + 그린 수 x 걸음)
//==============================================================================
#[cfg(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64"))]
fn process_k_rate_vector(periodic_wave: &PeriodicWave, destination: &mut [f32], virtual_read_index: f64, frequency: f32, rate_scale: f32) -> (usize, f64) {
	let periodic_wave_size = periodic_wave.periodic_wave_size;
	let read_index_mask = periodic_wave_size - 1;
	let increment = frequency * rate_scale;
	let wave_data = periodic_wave.get_wave_data(frequency);
	let (lower_wave_data, higher_wave_data) = periodic_wave.get_table_pair(&wave_data);
	let wave_size = periodic_wave_size as f32;
	let inverse_wave_size = 1.0 / periodic_wave_size as f32;
	let vector_increment = 4.0 * increment;
	#[cfg(target_arch = "aarch64")]
	let mut virtual_index_list = {
		let read_index = virtual_read_index as f32;
		[read_index + 0.0 * increment, read_index + increment, read_index + 2.0 * increment, read_index + 3.0 * increment]
	};
	#[cfg(not(target_arch = "aarch64"))]
	let mut virtual_index_list = [virtual_read_index as f32, (virtual_read_index + increment as f64) as f32, (virtual_read_index + (2.0 * increment) as f64) as f32, (virtual_read_index + (3.0 * increment) as f64) as f32];
	for virtual_index in virtual_index_list.iter_mut() {
		*virtual_index = wrap_virtual_index_f32(*virtual_index, wave_size, inverse_wave_size);
	}
	let mut frame_index = 0;
	for _ in 0..destination.len() / 4 {
		for (offset, virtual_index) in virtual_index_list.iter_mut().enumerate() {
			let read_index_0 = (*virtual_index as u32) & read_index_mask;
			let read_index_1 = (read_index_0 + 1) & read_index_mask;
			let interpolation_factor = *virtual_index - read_index_0 as f32;
			destination[frame_index + offset] = interpolate_linear(lower_wave_data, higher_wave_data, read_index_0 as usize, read_index_1 as usize, interpolation_factor, wave_data.table_interpolation_factor);
			*virtual_index = wrap_virtual_index_f32(*virtual_index + vector_increment, wave_size, inverse_wave_size);
		}
		frame_index += 4;
	}
	let virtual_read_index = virtual_read_index + (frame_index as f32 * increment) as f64;
	(frame_index, wrap_virtual_index(virtual_read_index, periodic_wave_size))
}

//==============================================================================
// k-rate 4 표본 묶음. (벡터가 없는 CPU 의 크롬은 묶음 없이 모두 스칼라로)
//==============================================================================
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86", target_arch = "x86_64")))]
fn process_k_rate_vector(_periodic_wave: &PeriodicWave, _destination: &mut [f32], virtual_read_index: f64, _frequency: f32, _rate_scale: f32) -> (usize, f64) {
	(0, virtual_read_index)
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 표 크기와 수는 표본율을 따르고, 첫 표의 가장 큰 크기는 1, 사인 표는 사인이다.
	//==============================================================================
	#[test]
	fn periodic_wave_tables_follow_chromium() {
		for (sample_rate, periodic_wave_size, number_of_ranges) in [(22050.0, 2048, 33), (44100.0, 4096, 36), (96000.0, 16384, 42)] {
			let periodic_wave = PeriodicWave::create_basic_waveform(sample_rate, OscillatorType::Sawtooth);
			assert_eq!(periodic_wave.periodic_wave_size, periodic_wave_size);
			assert_eq!(periodic_wave.number_of_ranges, number_of_ranges);
			assert_eq!(periodic_wave.band_limited_table_list.len(), number_of_ranges as usize);
			let maximum_value = periodic_wave.band_limited_table_list[0].iter().fold(0.0f32, |maximum_value, value| maximum_value.max(value.abs()));
			assert!((maximum_value - 1.0).abs() < 1e-6);
		}
		let sine_wave = PeriodicWave::create_basic_waveform(48000.0, OscillatorType::Sine);
		for (table_index, value) in sine_wave.band_limited_table_list[0].iter().enumerate() {
			assert!((*value as f64 - (std::f64::consts::TAU * table_index as f64 / 4096.0).sin()).abs() < 1e-6);
		}
		// 사각파 첫 표의 가운데는 1 / 1.17898, 삼각파의 꼭대기는 1 이다. (깁스 너울 꼭대기가 1)
		let square_wave = PeriodicWave::create_basic_waveform(44100.0, OscillatorType::Square);
		assert!((square_wave.band_limited_table_list[0][1024] as f64 - 1.0 / 1.1789798).abs() < 1e-3);
		let triangle_wave = PeriodicWave::create_basic_waveform(44100.0, OscillatorType::Triangle);
		assert!((triangle_wave.band_limited_table_list[0][1024] - 1.0).abs() < 1e-3);
	}

	//==============================================================================
	// 440 Hz 는 배음 40 개(표 17)와 32 개(표 18) 사이, 880 Hz 는 20 개와 16 개 사이다.
	//==============================================================================
	#[test]
	fn periodic_wave_culls_partials_by_frequency() {
		let periodic_wave = PeriodicWave::create_basic_waveform(44100.0, OscillatorType::Sawtooth);
		let wave_data = periodic_wave.get_wave_data(440.0);
		assert_eq!((wave_data.higher_index, wave_data.lower_index), (17, 18));
		assert!(wave_data.table_interpolation_factor > 0.0 && wave_data.table_interpolation_factor < 0.1);
		let wave_data = periodic_wave.get_wave_data(880.0);
		assert_eq!((wave_data.higher_index, wave_data.lower_index), (20, 21));
		// 표 17 은 40 번 배음까지 있고 41 번이 없다. (사인 계수를 안쪽 곱으로 되짚는다)
		let read_partial = |range_index: usize, harmonic_index: usize| -> f64 {
			let table = &periodic_wave.band_limited_table_list[range_index];
			let sum: f64 = table.iter().enumerate().map(|(table_index, value)| *value as f64 * (std::f64::consts::TAU * (harmonic_index * table_index) as f64 / 4096.0).sin()).sum();
			sum * 2.0 / 4096.0
		};
		assert!(read_partial(17, 40).abs() > 1e-3);
		assert!(read_partial(17, 41).abs() < 1e-6);
		assert!(read_partial(18, 32).abs() > 1e-3);
		assert!(read_partial(18, 33).abs() < 1e-6);
		// 표 33 에는 기본음만 남고, 그 위 두 표는 비었다.
		assert!(read_partial(33, 1).abs() > 0.5);
		assert!(read_partial(33, 2).abs() < 1e-6);
		assert!(periodic_wave.band_limited_table_list[34].iter().all(|value| *value == 0.0));
		// 아주 높거나 0 인 주파수는 끝 표로 자른다.
		assert_eq!(periodic_wave.get_wave_data(30000.0).higher_index, 35);
		assert_eq!(periodic_wave.get_wave_data(0.0).higher_index, 0);
	}

	//==============================================================================
	// 시작 앞은 0, 정지 프레임(올림)부터 0 이고 그 퀀텀에 끝난다.
	//==============================================================================
	#[test]
	fn oscillator_follows_start_and_stop_frames() {
		// (2 의 거듭제곱 표본율이라 시각 x 표본율이 딱 떨어진다)
		let sample_rate = 32768.0;
		let periodic_wave = Arc::new(PeriodicWave::create_basic_waveform(sample_rate as f32, OscillatorType::Square));
		let mut oscillator = OscillatorNode::new(periodic_wave, sample_rate, 200.0 / sample_rate, 300.5 / sample_rate);
		let mut output = [1.0f32; RENDER_QUANTUM_FRAMES];
		assert!(!oscillator.process(&mut output, 0, sample_rate));
		assert!(output.iter().all(|sample| *sample == 0.0));
		assert!(oscillator.process(&mut output, 128, sample_rate));
		assert!(output[..72].iter().all(|sample| *sample == 0.0));
		assert!(output[73] > 0.0);
		assert!(!oscillator.is_finished());
		assert!(oscillator.process(&mut output, 256, sample_rate));
		assert!(output[44] != 0.0);
		assert!(output[45..].iter().all(|sample| *sample == 0.0));
		assert!(oscillator.is_finished());
	}
}
