//==============================================================================
//! 표본율 바꾸기. (크로미엄 media/base/sinc_resampler.cc 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 크롬 decodeAudioData 는 푼 소리를 컨텍스트 표본율로 바꾼다. (Blink AudioBus::TryCreateBySampleRateConverting)
//!   채널마다 request_frames 64(커널 32 탭)로 SincResampler 를 만들고, 원본을 차례로 넘긴 뒤 모자라면 0 으로 채운다.
//!   결과 길이는 (int)(원본 길이 / (원본 표본율 / 새 표본율)) 이다.
//! - 합은 크롬이 고르는 길과 같은 차례로 더한다. (aarch64 는 NEON 4 줄, x86_64 는 AVX2 와 FMA 가 있으면 8 줄 FMA, 없으면 SSE 4 줄, 그 밖은 차례대로)
//!   크롬과 견준 기대값은 애플 실리콘(NEON) 맥에서 뽑았고, x86 의 두 갈래는 실기에서 돌려 보지 못했다.
//==============================================================================

/// 커널 크기. (request_frames 64 는 1.5 커널(96)보다 작아 kMinKernelSize)
const KERNEL_SIZE: usize = 32;
/// 커널 사이 오프셋 수. (kKernelOffsetCount)
const KERNEL_OFFSET_COUNT: usize = 32;
/// 한 번에 읽는 프레임 수. (Blink 가 넘기는 request_frames)
pub(crate) const REQUEST_FRAMES: usize = 64;

//==============================================================================
/// SincResampler. (스트리밍도 채널마다 하나를 두고 조각마다 부른다)
//==============================================================================
pub(crate) struct SincResampler {
	io_sample_rate_ratio: f64,
	virtual_source_index: f64,
	is_buffer_primed: bool,
	block_size: usize,
	/// r0 의 시작 자리. (input_buffer 안, r1 = 0, r2 = KERNEL_SIZE / 2)
	r0_offset: usize,
	r3_offset: usize,
	kernel_storage: Vec<f32>,
	input_buffer: Vec<f32>,
}

impl SincResampler {
	//==============================================================================
	/// 생성. (크롬 생성자와 InitializeKernel, Flush)
	//==============================================================================
	pub(crate) fn new(io_sample_rate_ratio: f64) -> SincResampler {
		let mut sinc_resampler = SincResampler {
			io_sample_rate_ratio,
			virtual_source_index: 0.0,
			is_buffer_primed: false,
			block_size: 0,
			r0_offset: 0,
			r3_offset: 0,
			kernel_storage: vec![0.0; KERNEL_SIZE * (KERNEL_OFFSET_COUNT + 1)],
			input_buffer: vec![0.0; REQUEST_FRAMES + KERNEL_SIZE],
		};
		sinc_resampler.update_regions(false);
		sinc_resampler.initialize_kernel();
		sinc_resampler
	}

	//==============================================================================
	/// 영역 다시 잡기. (UpdateRegions)
	//==============================================================================
	fn update_regions(&mut self, is_second_load: bool) {
		self.r0_offset = if is_second_load { KERNEL_SIZE } else { KERNEL_SIZE / 2 };
		self.r3_offset = self.r0_offset + REQUEST_FRAMES - KERNEL_SIZE;
		let r4_offset = self.r0_offset + REQUEST_FRAMES - KERNEL_SIZE / 2;
		self.block_size = r4_offset - KERNEL_SIZE / 2;
	}

	//==============================================================================
	/// 커널 짓기. (InitializeKernel, 블랙먼 창을 씌운 싱크)
	//==============================================================================
	fn initialize_kernel(&mut self) {
		const ALPHA: f64 = 0.16;
		const A0: f64 = 0.5 * (1.0 - ALPHA);
		const A1: f64 = 0.5;
		const A2: f64 = 0.5 * ALPHA;
		let mut sinc_scale_factor = if self.io_sample_rate_ratio > 1.0 { 1.0 / self.io_sample_rate_ratio } else { 1.0 };
		sinc_scale_factor *= 0.90;
		let signed_kernel_size = KERNEL_SIZE as i32;
		for offset_index in 0..=KERNEL_OFFSET_COUNT {
			let subsample_offset = offset_index as f32 / KERNEL_OFFSET_COUNT as f32;
			for kernel_index in 0..signed_kernel_size {
				let index = kernel_index as usize + offset_index * KERNEL_SIZE;
				let pre_sinc = std::f32::consts::PI * ((kernel_index - signed_kernel_size / 2) as f32 - subsample_offset);
				let x = (kernel_index as f32 - subsample_offset) / signed_kernel_size as f32;
				let window = (A0 - A1 * (2.0 * std::f64::consts::PI * x as f64).cos() + A2 * (4.0 * std::f64::consts::PI * x as f64).cos()) as f32;
				let sinc = if pre_sinc != 0.0 { (sinc_scale_factor * pre_sinc as f64).sin() / pre_sinc as f64 } else { sinc_scale_factor };
				self.kernel_storage[index] = (window as f64 * sinc) as f32;
			}
		}
	}

	//==============================================================================
	/// 바꾸기. (Resample, read 는 받은 칸을 원본으로 채운다)
	//==============================================================================
	pub(crate) fn resample(&mut self, destination: &mut [f32], read: &mut dyn FnMut(&mut [f32])) {
		let mut remaining_frame_count = destination.len();
		let mut destination_index = 0;
		if !self.is_buffer_primed && remaining_frame_count > 0 {
			let r0_offset = self.r0_offset;
			read(&mut self.input_buffer[r0_offset..r0_offset + REQUEST_FRAMES]);
			self.is_buffer_primed = true;
		}
		while remaining_frame_count > 0 {
			while self.virtual_source_index < self.block_size as f64 {
				let source_index = self.virtual_source_index as usize;
				let virtual_offset_index = (self.virtual_source_index - source_index as f64) * KERNEL_OFFSET_COUNT as f64;
				let offset_index = virtual_offset_index as usize;
				let starting_index = offset_index * KERNEL_SIZE;
				let kernel1 = &self.kernel_storage[starting_index..starting_index + KERNEL_SIZE];
				let kernel2 = &self.kernel_storage[starting_index + KERNEL_SIZE..starting_index + 2 * KERNEL_SIZE];
				let input = &self.input_buffer[source_index..source_index + KERNEL_SIZE];
				let kernel_interpolation_factor = virtual_offset_index - offset_index as f64;
				destination[destination_index] = convolve(input, kernel1, kernel2, kernel_interpolation_factor);
				destination_index += 1;
				self.virtual_source_index += self.io_sample_rate_ratio;
				remaining_frame_count -= 1;
				if remaining_frame_count == 0 {
					return;
				}
			}
			self.virtual_source_index -= self.block_size as f64;
			// r3 의 커널 크기만큼을 r1(맨 앞)으로 옮긴다.
			let r3_offset = self.r3_offset;
			self.input_buffer.copy_within(r3_offset..r3_offset + KERNEL_SIZE, 0);
			if self.r0_offset == KERNEL_SIZE / 2 {
				self.update_regions(true);
			}
			let r0_offset = self.r0_offset;
			read(&mut self.input_buffer[r0_offset..r0_offset + REQUEST_FRAMES]);
		}
	}
}

//==============================================================================
/// 합성곱. (크롬이 그 CPU 에서 고르는 Convolve_* 와 같은 차례로 더한다)
//==============================================================================
#[cfg(target_arch = "aarch64")]
fn convolve(input: &[f32], kernel1: &[f32], kernel2: &[f32], kernel_interpolation_factor: f64) -> f32 {
	convolve_four_lanes(input, kernel1, kernel2, kernel_interpolation_factor)
}

//==============================================================================
/// 합성곱. (x86_64: AVX2 와 FMA 가 있으면 8 줄 묶음, 없으면 4 줄 합)
//==============================================================================
#[cfg(target_arch = "x86_64")]
fn convolve(input: &[f32], kernel1: &[f32], kernel2: &[f32], kernel_interpolation_factor: f64) -> f32 {
	if std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma") {
		convolve_eight_lanes_fused(input, kernel1, kernel2, kernel_interpolation_factor)
	}
	else {
		convolve_four_lanes(input, kernel1, kernel2, kernel_interpolation_factor)
	}
}

//==============================================================================
/// 합성곱. (그 밖의 CPU: 두 합을 차례로 더한 뒤 섞는다)
//==============================================================================
#[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
fn convolve(input: &[f32], kernel1: &[f32], kernel2: &[f32], kernel_interpolation_factor: f64) -> f32 {
	let mut sum1 = 0.0f32;
	let mut sum2 = 0.0f32;
	for index in 0..input.len() {
		sum1 += input[index] * kernel1[index];
		sum2 += input[index] * kernel2[index];
	}
	((1.0 - kernel_interpolation_factor) * sum1 as f64 + kernel_interpolation_factor * sum2 as f64) as f32
}

//==============================================================================
/// 4 줄 합. (Convolve_SSE, Convolve_NEON: 줄마다 곱해 더하고, 두 합을 (1 - f), f 로 섞은 뒤 (0 + 2) + (1 + 3))
//==============================================================================
#[cfg(any(target_arch = "aarch64", target_arch = "x86_64"))]
fn convolve_four_lanes(input: &[f32], kernel1: &[f32], kernel2: &[f32], kernel_interpolation_factor: f64) -> f32 {
	let mut sum_lane_list1 = [0.0f32; 4];
	let mut sum_lane_list2 = [0.0f32; 4];
	let mut index = 0;
	while index < input.len() {
		for lane in 0..4 {
			sum_lane_list1[lane] += input[index + lane] * kernel1[index + lane];
			sum_lane_list2[lane] += input[index + lane] * kernel2[index + lane];
		}
		index += 4;
	}
	let factor1 = (1.0 - kernel_interpolation_factor) as f32;
	let factor2 = kernel_interpolation_factor as f32;
	let mut mixed_lane_list = [0.0f32; 4];
	for lane in 0..4 {
		mixed_lane_list[lane] = sum_lane_list1[lane] * factor1 + sum_lane_list2[lane] * factor2;
	}
	(mixed_lane_list[2] + mixed_lane_list[0]) + (mixed_lane_list[3] + mixed_lane_list[1])
}

//==============================================================================
/// 8 줄 FMA 합. (Convolve_AVX2: 줄마다 fmadd, 위아래 128 비트를 더한 4 줄을 SSE 처럼 섞고 모은다)
//==============================================================================
#[cfg(target_arch = "x86_64")]
fn convolve_eight_lanes_fused(input: &[f32], kernel1: &[f32], kernel2: &[f32], kernel_interpolation_factor: f64) -> f32 {
	let mut sum_lane_list1 = [0.0f32; 8];
	let mut sum_lane_list2 = [0.0f32; 8];
	let mut index = 0;
	while index < input.len() {
		for lane in 0..8 {
			sum_lane_list1[lane] = input[index + lane].mul_add(kernel1[index + lane], sum_lane_list1[lane]);
			sum_lane_list2[lane] = input[index + lane].mul_add(kernel2[index + lane], sum_lane_list2[lane]);
		}
		index += 8;
	}
	let factor1 = (1.0 - kernel_interpolation_factor) as f32;
	let factor2 = kernel_interpolation_factor as f32;
	let mut mixed_lane_list = [0.0f32; 4];
	for lane in 0..4 {
		let half_sum1 = sum_lane_list1[lane] + sum_lane_list1[lane + 4];
		let half_sum2 = sum_lane_list2[lane] + sum_lane_list2[lane + 4];
		mixed_lane_list[lane] = half_sum1 * factor1 + half_sum2 * factor2;
	}
	(mixed_lane_list[2] + mixed_lane_list[0]) + (mixed_lane_list[3] + mixed_lane_list[1])
}

//==============================================================================
/// 한 채널의 표본율 바꾸기. (Blink TryCreateBySampleRateConverting 의 채널 하나, 같은 표본율이면 그대로)
//==============================================================================
pub(crate) fn resample_channel(source: &[f32], source_sample_rate: f64, destination_sample_rate: f64) -> Vec<f32> {
	if source_sample_rate == destination_sample_rate {
		return source.to_vec();
	}
	let sample_rate_ratio = source_sample_rate / destination_sample_rate;
	let destination_length = (source.len() as f64 / sample_rate_ratio) as usize;
	let mut destination = vec![0.0f32; destination_length];
	if source.iter().all(|sample| *sample == 0.0) {
		return destination;
	}
	let mut sinc_resampler = SincResampler::new(sample_rate_ratio);
	let mut source_position = 0;
	let mut read = |chunk: &mut [f32]| {
		let copy_count = chunk.len().min(source.len() - source_position);
		chunk[..copy_count].copy_from_slice(&source[source_position..source_position + copy_count]);
		chunk[copy_count..].fill(0.0);
		source_position += copy_count;
	};
	sinc_resampler.resample(&mut destination, &mut read);
	destination
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 다시 뽑은 길이가 블링크와 같다. (48000 에서 44100, 8000 에서 44100, 같은 표본율은 그대로)
	//==============================================================================
	#[test]
	fn resample_length_matches_blink() {
		assert_eq!(resample_channel(&vec![0.5; 2880], 48000.0, 44100.0).len(), 2645);
		assert_eq!(resample_channel(&vec![0.5; 2000], 8000.0, 44100.0).len(), 11025);
		assert_eq!(resample_channel(&vec![0.5; 12000], 48000.0, 44100.0).len(), 11024);
		assert_eq!(resample_channel(&[0.25, 0.5], 44100.0, 44100.0), vec![0.25, 0.5]);
	}

	//==============================================================================
	/// 고른 값은 다시 뽑아도 그 값이다. (0.01 안)
	//==============================================================================
	#[test]
	fn resample_keeps_constant_level() {
		let output = resample_channel(&vec![0.5; 4800], 48000.0, 44100.0);
		assert!((output[2000] - 0.5).abs() < 0.01, "{}", output[2000]);
	}
}
