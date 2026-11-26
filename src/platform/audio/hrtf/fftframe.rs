//==============================================================================
//! HRTF 의 실수 FFT, 주파수 틀, FFT 합성곱. (크로미엄 third_party/blink/renderer/platform/audio/rustfft_ffi.rs, fft_frame.cc, fft_convolver.cc,
//! vector_math.cc 와 mac/vector_math_mac.h 를 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 크롬(158)의 FFTFrame 은 모든 플랫폼에서 rustfft 6.4 를 부른다. (rustfft_ffi.rs) 엔진도 같은 판, 같은 기능(avx, sse, neon)의 rustfft 를
//!   같은 꼴로 부른다. 짝수 크기 N 은 N/2 복소 FFT 에 실수 표본 둘씩을 넣고 회전 인자(0.5 W^k, f64 로 셈해 f32 로)로 펼친다.
//!   값은 교과서 DFT 크기 그대로이고 역 FFT 가 2/N 을 곱한다. 계획(plan)은 크기마다 한 번 지어 나눠 쓴다.
//! - 주파수 틀(FFTFrame)의 꼴: real[0] 은 직류, imag[0] 은 나이퀴스트, k = 1 ~ N/2 - 1 은 X[k] 다. 크롬은 틀마다 FFT 를 들지만
//!   엔진은 커널이 값만 들고 FFT(RealFft)는 셈하는 쪽이 든다. (값은 같다)
//! - 크기 보간(InterpolateFrequencyComponents)은 크롬처럼 log10 과 pow 를 fdlibm 으로, 크기와 위상(std::abs, std::arg)과 극좌표(std::polar)는
//!   시스템 libm 의 hypot, atan2, cos, sin 으로 셈한다.
//! - 곱하기(Zvmul): 크롬 맥은 vDSP_zvmul 이고, 애플 실리콘에서 재 보니 실수 = fma(-ai, bi, ar br), 허수 = fma(ai, br, ar bi) 로 곱셈과 덧셈을 합친다.
//!   그 밖의 크롬(x86 SSE, AVX 와 NEON)은 따로 곱해 더한다. 엔진도 애플 실리콘 맥만 합친다. (인텔 맥의 vDSP 는 재 보지 못해 따로 한다)
//==============================================================================
use super::fdlibm;
use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// 앞, 뒤 계획 한 쌍.
type FftPair = (Arc<dyn Fft<f32>>, Arc<dyn Fft<f32>>);

//==============================================================================
/// 크기마다의 계획. (rustfft_ffi.rs get_fft, 처음 부를 때 지어 나눠 쓴다)
///
/// # 인자
/// - `size`: 복소 FFT 크기.
///
/// # 돌려줌
/// (앞 계획, 뒤 계획).
//==============================================================================
fn get_fft(size: usize) -> FftPair {
	static CACHE: OnceLock<RwLock<HashMap<usize, FftPair>>> = OnceLock::new();
	let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));
	if let Some(pair) = cache.read().unwrap().get(&size) {
		return pair.clone();
	}
	let mut planner = FftPlanner::new();
	let forward = planner.plan_fft_forward(size);
	let inverse = planner.plan_fft_inverse(size);
	let mut cache_write = cache.write().unwrap();
	cache_write.entry(size).or_insert((forward, inverse)).clone()
}

//==============================================================================
/// 실수 FFT 의 주파수 값. (FFTFrame 의 real_data, imag_data, 칸 수는 N/2)
//==============================================================================
pub(super) struct FftFrame {
	pub(super) real_data: Vec<f32>,
	pub(super) imag_data: Vec<f32>,
}

impl FftFrame {
	//==============================================================================
	/// 0 으로 채운 틀.
	///
	/// # 인자
	/// - `fft_size`: FFT 크기. (짝수)
	///
	/// # 돌려줌
	/// 틀.
	//==============================================================================
	pub(super) fn new(fft_size: usize) -> FftFrame {
		let packed_size = fft_size.div_ceil(2);
		FftFrame {
			real_data: vec![0.0; packed_size],
			imag_data: vec![0.0; packed_size],
		}
	}

	//==============================================================================
	/// FFT 크기.
	///
	/// # 돌려줌
	/// 칸 수의 두 배.
	//==============================================================================
	fn get_fft_size(&self) -> usize {
		self.real_data.len() * 2
	}

	//==============================================================================
	/// 두 틀 사이를 섞은 틀. (FFTFrame::CreateInterpolatedFrame, 크기는 데시벨로, 위상은 군지연으로 섞고 시간 영역 뒤 반을 0 으로)
	///
	/// # 인자
	/// - `frame1`: x = 0 쪽 틀.
	/// - `frame2`: x = 1 쪽 틀.
	/// - `x`: 섞는 비. (0 ~ 1)
	/// - `real_fft`: 이 크기의 FFT.
	///
	/// # 돌려줌
	/// 섞은 틀.
	//==============================================================================
	pub(super) fn create_interpolated_frame(frame1: &FftFrame, frame2: &FftFrame, x: f64, real_fft: &mut RealFft) -> FftFrame {
		let fft_size = frame1.get_fft_size();
		let mut new_frame = FftFrame::new(fft_size);
		new_frame.interpolate_frequency_components(frame1, frame2, x);
		// 시간 영역의 뒤 반은 0 이어야 원형 합성곱이 겹치지 않는다.
		let mut buffer = vec![0.0f32; fft_size];
		real_fft.do_inverse_fft(&new_frame, &mut buffer);
		buffer[fft_size / 2..].fill(0.0);
		real_fft.do_fft(&buffer, &mut new_frame);
		new_frame
	}

	//==============================================================================
	/// 크기와 위상 섞기. (FFTFrame::InterpolateFrequencyComponents)
	/// - 크기는 데시벨로 섞되, 한쪽이 훨씬 작고 0 dB 밑이면 그쪽 비를 0.75 제곱으로 줄여 높은 주파수의 골을 남긴다.
	/// - 위상은 칸 사이 위상차(군지연)를 펴서 섞고 쌓은 뒤 -π ~ π 로 접는다.
	///
	/// # 인자
	/// - `frame1`: x = 0 쪽 틀.
	/// - `frame2`: x = 1 쪽 틀.
	/// - `interpolation`: 섞는 비.
	//==============================================================================
	fn interpolate_frequency_components(&mut self, frame1: &FftFrame, frame2: &FftFrame, interpolation: f64) {
		use std::f64::consts::{PI, TAU};
		let s1_base = 1.0 - interpolation;
		let s2_base = interpolation;
		let mut phase_accumulation = 0.0f64;
		let mut last_phase1 = 0.0f64;
		let mut last_phase2 = 0.0f64;
		self.real_data[0] = (s1_base * frame1.real_data[0] as f64 + s2_base * frame2.real_data[0] as f64) as f32;
		self.imag_data[0] = (s1_base * frame1.imag_data[0] as f64 + s2_base * frame2.imag_data[0] as f64) as f32;
		for index in 1..frame1.real_data.len() {
			let (real1, imag1) = (frame1.real_data[index] as f64, frame1.imag_data[index] as f64);
			let (real2, imag2) = (frame2.real_data[index] as f64, frame2.imag_data[index] as f64);
			let magnitude1 = real1.hypot(imag1);
			let magnitude2 = real2.hypot(imag2);
			// 크기를 데시벨로 섞는다.
			let decibel_magnitude1 = 20.0 * fdlibm::log10(magnitude1);
			let decibel_magnitude2 = 20.0 * fdlibm::log10(magnitude2);
			let mut s1 = s1_base;
			let mut s2 = s2_base;
			let decibel_magnitude_difference = decibel_magnitude1 - decibel_magnitude2;
			// 높은 주파수의 골을 남기려는 경험값.
			let threshold = if index > 16 { 5.0 } else { 2.0 };
			if decibel_magnitude_difference < -threshold && decibel_magnitude1 < 0.0 {
				s1 = fdlibm::pow(s1, 0.75);
				s2 = 1.0 - s1;
			}
			else if decibel_magnitude_difference > threshold && decibel_magnitude2 < 0.0 {
				s2 = fdlibm::pow(s2, 0.75);
				s1 = 1.0 - s2;
			}
			let decibel_magnitude = s1 * decibel_magnitude1 + s2 * decibel_magnitude2;
			let magnitude = fdlibm::pow(10.0, 0.05 * decibel_magnitude);
			// 위상.
			let phase1 = imag1.atan2(real1);
			let phase2 = imag2.atan2(real2);
			let mut delta_phase1 = phase1 - last_phase1;
			let mut delta_phase2 = phase2 - last_phase2;
			last_phase1 = phase1;
			last_phase2 = phase2;
			// 위상차를 편다.
			if delta_phase1 > PI {
				delta_phase1 -= TAU;
			}
			if delta_phase1 < -PI {
				delta_phase1 += TAU;
			}
			if delta_phase2 > PI {
				delta_phase2 -= TAU;
			}
			if delta_phase2 < -PI {
				delta_phase2 += TAU;
			}
			// 군지연을 섞는다.
			let delta_phase_blend = if delta_phase1 - delta_phase2 > PI {
				s1 * delta_phase1 + s2 * (TAU + delta_phase2)
			}
			else if delta_phase2 - delta_phase1 > PI {
				s1 * (TAU + delta_phase1) + s2 * delta_phase2
			}
			else {
				s1 * delta_phase1 + s2 * delta_phase2
			};
			phase_accumulation += delta_phase_blend;
			if phase_accumulation > PI {
				phase_accumulation -= TAU;
			}
			if phase_accumulation < -PI {
				phase_accumulation += TAU;
			}
			let (real, imag) = polar(magnitude, phase_accumulation);
			self.real_data[index] = real as f32;
			self.imag_data[index] = imag as f32;
		}
	}

	//==============================================================================
	/// 평균 군지연을 빼고 그 값을 돌려준다. (FFTFrame::ExtractAverageGroupDelay, 크기로 무게를 단 위상차의 평균, 20 표본은 앞머리로 남긴다)
	/// - 0 번 칸(직류와 나이퀴스트를 한 복소수로 친 것)도 크롬처럼 평균에 넣고, 끝에 직류를 0 으로 둔다.
	///
	/// # 돌려줌
	/// 뺀 지연. (표본)
	//==============================================================================
	pub(super) fn extract_average_group_delay(&mut self) -> f64 {
		use std::f64::consts::{PI, TAU};
		let mut average_sum = 0.0f64;
		let mut weight_sum = 0.0f64;
		let mut last_phase = 0.0f64;
		let sample_phase_delay = TAU / self.get_fft_size() as f64;
		for index in 0..self.real_data.len() {
			let (real, imag) = (self.real_data[index] as f64, self.imag_data[index] as f64);
			let magnitude = real.hypot(imag);
			let phase = imag.atan2(real);
			let mut delta_phase = phase - last_phase;
			last_phase = phase;
			if delta_phase < -PI {
				delta_phase += TAU;
			}
			if delta_phase > PI {
				delta_phase -= TAU;
			}
			average_sum += magnitude * delta_phase;
			weight_sum += magnitude;
		}
		// 군지연은 주파수에 대한 위상차의 부호를 뒤집은 것이다.
		let average = average_sum / weight_sum;
		let mut average_sample_delay = -average / sample_phase_delay;
		// 앞머리로 20 표본을 남긴다.
		if average_sample_delay > 20.0 {
			average_sample_delay -= 20.0;
		}
		self.add_constant_group_delay(-average_sample_delay);
		self.real_data[0] = 0.0;
		average_sample_delay
	}

	//==============================================================================
	/// 고른 군지연 더하기. (FFTFrame::AddConstantGroupDelay, 칸 i 의 위상에 i x 지연 x 2π/N 을 뺀다)
	///
	/// # 인자
	/// - `sample_frame_delay`: 더할 지연. (표본)
	//==============================================================================
	fn add_constant_group_delay(&mut self, sample_frame_delay: f64) {
		let sample_phase_delay = std::f64::consts::TAU / self.get_fft_size() as f64;
		let phase_adjustment = -sample_frame_delay * sample_phase_delay;
		for index in 1..self.real_data.len() {
			let (real, imag) = (self.real_data[index] as f64, self.imag_data[index] as f64);
			let magnitude = real.hypot(imag);
			let mut phase = imag.atan2(real);
			phase += index as f64 * phase_adjustment;
			let (new_real, new_imag) = polar(magnitude, phase);
			self.real_data[index] = new_real as f32;
			self.imag_data[index] = new_imag as f32;
		}
	}

	//==============================================================================
	/// 다른 틀을 곱하기. (FFTFrame::Multiply, 복소 곱 뒤 0 번 칸의 직류와 나이퀴스트는 따로 곱한다)
	///
	/// # 인자
	/// - `frame`: 곱할 틀. (커널)
	//==============================================================================
	pub(super) fn multiply(&mut self, frame: &FftFrame) {
		let real0 = self.real_data[0];
		let imag0 = self.imag_data[0];
		for index in 0..self.real_data.len() {
			let (real, imag) = multiply_complex(self.real_data[index], self.imag_data[index], frame.real_data[index], frame.imag_data[index]);
			self.real_data[index] = real;
			self.imag_data[index] = imag;
		}
		self.real_data[0] = real0 * frame.real_data[0];
		self.imag_data[0] = imag0 * frame.imag_data[0];
	}
}

//==============================================================================
/// 극좌표에서 복소수로. (libc++ std::polar, NaN 과 무한, 음의 크기 처리까지)
///
/// # 인자
/// - `magnitude`: 크기.
/// - `phase`: 위상. (라디안)
///
/// # 돌려줌
/// (실수, 허수).
//==============================================================================
fn polar(magnitude: f64, phase: f64) -> (f64, f64) {
	if magnitude.is_nan() || magnitude.is_sign_negative() {
		return (f64::NAN, f64::NAN);
	}
	if phase.is_nan() {
		if magnitude.is_infinite() {
			return (magnitude, phase);
		}
		return (phase, phase);
	}
	if phase.is_infinite() {
		if magnitude.is_infinite() {
			return (magnitude, f64::NAN);
		}
		return (f64::NAN, f64::NAN);
	}
	let mut real = magnitude * phase.cos();
	if real.is_nan() {
		real = 0.0;
	}
	let mut imag = magnitude * phase.sin();
	if imag.is_nan() {
		imag = 0.0;
	}
	(real, imag)
}

//==============================================================================
/// 복소 곱 하나. (vector_math::Zvmul, 애플 실리콘 맥은 vDSP_zvmul 처럼 곱셈과 덧셈을 합친다)
///
/// # 인자
/// - `real1`: 왼쪽 실수.
/// - `imag1`: 왼쪽 허수.
/// - `real2`: 오른쪽 실수.
/// - `imag2`: 오른쪽 허수.
///
/// # 돌려줌
/// (실수, 허수).
//==============================================================================
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn multiply_complex(real1: f32, imag1: f32, real2: f32, imag2: f32) -> (f32, f32) {
	((-imag1).mul_add(imag2, real1 * real2), imag1.mul_add(real2, real1 * imag2))
}

//==============================================================================
/// 복소 곱 하나. (vector_math::Zvmul 의 SSE, AVX, NEON 과 스칼라 판, 따로 곱해 더한다)
///
/// # 인자
/// - `real1`: 왼쪽 실수.
/// - `imag1`: 왼쪽 허수.
/// - `real2`: 오른쪽 실수.
/// - `imag2`: 오른쪽 허수.
///
/// # 돌려줌
/// (실수, 허수).
//==============================================================================
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn multiply_complex(real1: f32, imag1: f32, real2: f32, imag2: f32) -> (f32, f32) {
	(real1 * real2 - imag1 * imag2, real1 * imag2 + imag1 * real2)
}

//==============================================================================
/// 곱해 더하기 하나. (vector_math::Vsma 의 dest + scale x source, 애플 실리콘 맥은 vDSP_vsma 처럼 합친다)
///
/// # 인자
/// - `source`: 곱할 값.
/// - `scale`: 배율.
/// - `destination`: 더할 값.
///
/// # 돌려줌
/// destination + scale x source.
//==============================================================================
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(super) fn multiply_add(source: f32, scale: f32, destination: f32) -> f32 {
	scale.mul_add(source, destination)
}

//==============================================================================
/// 곱해 더하기 하나. (vector_math::Vsma 의 SSE, AVX, NEON 과 스칼라 판, 따로 곱해 더한다)
///
/// # 인자
/// - `source`: 곱할 값.
/// - `scale`: 배율.
/// - `destination`: 더할 값.
///
/// # 돌려줌
/// destination + scale x source.
//==============================================================================
#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub(super) fn multiply_add(source: f32, scale: f32, destination: f32) -> f32 {
	destination + scale * source
}

//==============================================================================
/// 짝수 크기 실수 FFT. (rustfft_ffi.rs 의 RustFft, Even 갈래)
//==============================================================================
pub(super) struct RealFft {
	size: usize,
	half_size: usize,
	limit: usize,
	middle_index: Option<usize>,
	fft_forward: Arc<dyn Fft<f32>>,
	fft_inverse: Arc<dyn Fft<f32>>,
	/// 펼치는 회전 인자. (0.5 W^k, k = 0 ~ N/2 - 1)
	twiddle_list: Vec<Complex<f32>>,
	complex_data: Vec<Complex<f32>>,
	scratch: Vec<Complex<f32>>,
	/// 역 FFT 의 배율. (1 / (N/2))
	scale: f32,
}

impl RealFft {
	//==============================================================================
	/// 생성. (rustfft_new 의 짝수 갈래)
	///
	/// # 인자
	/// - `size`: FFT 크기. (2 이상의 짝수)
	///
	/// # 돌려줌
	/// FFT.
	//==============================================================================
	pub(super) fn new(size: usize) -> RealFft {
		let half_size = size / 2;
		let (fft_forward, fft_inverse) = get_fft(half_size);
		let scratch_length = fft_forward.get_inplace_scratch_len().max(fft_inverse.get_inplace_scratch_len());
		let twiddle_list = (0..half_size)
			.map(|k| {
				let angle = -2.0 * std::f64::consts::PI * (k as f64) / (size as f64);
				let (sine, cosine) = angle.sin_cos();
				Complex::new((0.5 * cosine) as f32, (0.5 * sine) as f32)
			})
			.collect();
		RealFft {
			size,
			half_size,
			limit: (half_size - 1) / 2,
			middle_index: if half_size % 2 == 0 { Some(half_size / 2) } else { None },
			fft_forward,
			fft_inverse,
			twiddle_list,
			complex_data: vec![Complex::new(0.0, 0.0); half_size],
			scratch: vec![Complex::new(0.0, 0.0); scratch_length],
			scale: 1.0 / (half_size as f32),
		}
	}

	//==============================================================================
	/// 앞 FFT. (do_fft_even 과 reconstruct_real_fft_even: 표본 둘씩을 복소수 하나로 넣어 반 크기 FFT 뒤 짝, 홀 부분을 회전 인자로 합친다)
	///
	/// # 인자
	/// - `data`: 시간 영역 표본. (size 개)
	/// - `frame`: 받을 틀.
	//==============================================================================
	pub(super) fn do_fft(&mut self, data: &[f32], frame: &mut FftFrame) {
		for (index, value) in self.complex_data.iter_mut().enumerate() {
			*value = Complex::new(data[2 * index], data[2 * index + 1]);
		}
		self.fft_forward.process_with_scratch(&mut self.complex_data, &mut self.scratch);
		let z = &self.complex_data;
		let half_size = self.half_size;
		let real_data = &mut frame.real_data;
		let imag_data = &mut frame.imag_data;
		// 0 번 칸에 직류와 나이퀴스트.
		real_data[0] = z[0].re + z[0].im;
		imag_data[0] = z[0].re - z[0].im;
		for k in 1..=self.limit {
			let z_k = z[k];
			let z_mk = z[half_size - k];
			// F_even[k] = 0.5 (Z[k] + Z*[N-k]), F_odd[k] = 0.5/i (Z[k] - Z*[N-k])
			let f_even = Complex::new(0.5 * (z_k.re + z_mk.re), 0.5 * (z_k.im - z_mk.im));
			let f_odd_unscaled = Complex::new(z_k.im + z_mk.im, -(z_k.re - z_mk.re));
			let w_f_odd = self.twiddle_list[k] * f_odd_unscaled;
			// X[k] = F_even[k] + W^k F_odd[k], X[N-k] = (F_even[k] - W^k F_odd[k])*
			let x_k = f_even + w_f_odd;
			let x_mk_conjugate = f_even - w_f_odd;
			real_data[k] = x_k.re;
			imag_data[k] = x_k.im;
			real_data[half_size - k] = x_mk_conjugate.re;
			imag_data[half_size - k] = -x_mk_conjugate.im;
		}
		if let Some(k) = self.middle_index {
			let z_k = z[k];
			real_data[k] = z_k.re;
			imag_data[k] = -z_k.im;
		}
	}

	//==============================================================================
	/// 역 FFT. (prepare_inverse_fft_even 과 do_inverse_fft_even: 짝, 홀 부분을 되돌려 반 크기 역 FFT 뒤 배율을 곱한다)
	///
	/// # 인자
	/// - `frame`: 주파수 틀.
	/// - `data`: 받을 시간 영역 표본. (size 개)
	//==============================================================================
	pub(super) fn do_inverse_fft(&mut self, frame: &FftFrame, data: &mut [f32]) {
		let half_size = self.half_size;
		let real_data = &frame.real_data;
		let imag_data = &frame.imag_data;
		let z = &mut self.complex_data;
		z[0] = Complex::new(0.5 * (real_data[0] + imag_data[0]), 0.5 * (real_data[0] - imag_data[0]));
		for k in 1..=self.limit {
			let x_k = Complex::new(real_data[k], imag_data[k]);
			let x_mk_conjugate = Complex::new(real_data[half_size - k], -imag_data[half_size - k]);
			let f_even = (x_k + x_mk_conjugate) * 0.5;
			let x_difference = x_k - x_mk_conjugate;
			// 역은 회전 인자의 켤레를 곱한다.
			let f_odd = self.twiddle_list[k].conj() * x_difference;
			let i_f_odd = Complex::new(-f_odd.im, f_odd.re);
			z[k] = f_even + i_f_odd;
			z[half_size - k] = (f_even - i_f_odd).conj();
		}
		if let Some(k) = self.middle_index {
			z[k] = Complex::new(real_data[k], -imag_data[k]);
		}
		self.fft_inverse.process_with_scratch(&mut self.complex_data, &mut self.scratch);
		for (index, value) in self.complex_data.iter().enumerate() {
			data[2 * index] = value.re * self.scale;
			data[2 * index + 1] = value.im * self.scale;
		}
	}

	//==============================================================================
	/// 0 을 덧대 앞 FFT. (FFTFrame::DoPaddedFFT)
	///
	/// # 인자
	/// - `data`: 표본. (size 개 이하)
	/// - `frame`: 받을 틀.
	//==============================================================================
	pub(super) fn do_padded_fft(&mut self, data: &[f32], frame: &mut FftFrame) {
		let mut padded_response = vec![0.0f32; self.size];
		padded_response[..data.len()].copy_from_slice(data);
		self.do_fft(&padded_response, frame);
	}
}

//==============================================================================
/// FFT 합성곱. (FFTConvolver, 반 크기마다 겹쳐 더하기. 입력 칸의 뒤 반은 늘 0 이다)
//==============================================================================
pub(super) struct FftConvolver {
	real_fft: RealFft,
	frame: FftFrame,
	read_write_index: usize,
	input_buffer: Vec<f32>,
	output_buffer: Vec<f32>,
	last_overlap_buffer: Vec<f32>,
}

impl FftConvolver {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `fft_size`: FFT 크기.
	///
	/// # 돌려줌
	/// 합성곱.
	//==============================================================================
	pub(super) fn new(fft_size: usize) -> FftConvolver {
		FftConvolver {
			real_fft: RealFft::new(fft_size),
			frame: FftFrame::new(fft_size),
			read_write_index: 0,
			input_buffer: vec![0.0; fft_size],
			output_buffer: vec![0.0; fft_size],
			last_overlap_buffer: vec![0.0; fft_size / 2],
		}
	}

	//==============================================================================
	/// 표본을 넣고 같은 수를 꺼낸다. (FFTConvolver::Process, 그 자리에서 바꾼다. 반 크기가 차면 FFT, 커널 곱, 역 FFT 뒤 앞 반에 지난 겹침을 더한다)
	/// - 크롬은 입력과 출력이 같은 칸일 수 있게 조각마다 넣은 뒤 꺼낸다. 여기는 그 꼴 그대로 한 칸에서 한다.
	///
	/// # 인자
	/// - `fft_kernel`: 커널 틀.
	/// - `data`: 넣을 표본. (꺼낸 표본으로 바뀐다)
	//==============================================================================
	pub(super) fn process(&mut self, fft_kernel: &FftFrame, data: &mut [f32]) {
		let half_size = self.input_buffer.len() / 2;
		let mut offset = 0;
		while offset < data.len() {
			let frame_count = (data.len() - offset).min(half_size - self.read_write_index);
			let read_write_index = self.read_write_index;
			self.input_buffer[read_write_index..read_write_index + frame_count].copy_from_slice(&data[offset..offset + frame_count]);
			data[offset..offset + frame_count].copy_from_slice(&self.output_buffer[read_write_index..read_write_index + frame_count]);
			offset += frame_count;
			self.read_write_index += frame_count;
			if self.read_write_index == half_size {
				self.real_fft.do_fft(&self.input_buffer, &mut self.frame);
				self.frame.multiply(fft_kernel);
				self.real_fft.do_inverse_fft(&self.frame, &mut self.output_buffer);
				for (output, overlap) in self.output_buffer[..half_size].iter_mut().zip(self.last_overlap_buffer.iter()) {
					*output += *overlap;
				}
				self.last_overlap_buffer.copy_from_slice(&self.output_buffer[half_size..]);
				self.read_write_index = 0;
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 앞 FFT 뒤 역 FFT 는 처음 표본이다. (f32 셈 오차 안) 직류와 나이퀴스트는 0 번 칸에 든다.
	//==============================================================================
	#[test]
	fn real_fft_round_trip_and_packing() {
		for size in [2usize, 8, 256, 512, 1024] {
			let mut real_fft = RealFft::new(size);
			let data: Vec<f32> = (0..size).map(|index| ((index * 37 % 101) as f32 - 50.0) / 64.0).collect();
			let mut frame = FftFrame::new(size);
			real_fft.do_fft(&data, &mut frame);
			let sum: f32 = data.iter().sum();
			let alternating_sum: f32 = data.iter().enumerate().map(|(index, value)| if index % 2 == 0 { *value } else { -*value }).sum();
			assert!((frame.real_data[0] - sum).abs() < 1e-3, "{} 직류", size);
			assert!((frame.imag_data[0] - alternating_sum).abs() < 1e-3, "{} 나이퀴스트", size);
			let mut output = vec![0.0f32; size];
			real_fft.do_inverse_fft(&frame, &mut output);
			for (left, right) in data.iter().zip(output.iter()) {
				assert!((left - right).abs() < 1e-5, "{}: {} / {}", size, left, right);
			}
		}
	}

	//==============================================================================
	/// 합성곱은 직접 셈한 선형 합성곱을 반 크기만큼 늦춘 것이다. (지연 FFT 크기 / 2)
	//==============================================================================
	#[test]
	fn fft_convolver_matches_direct_convolution() {
		let fft_size = 64;
		let half_size = fft_size / 2;
		let impulse_response: Vec<f32> = (0..half_size).map(|index| (index as f32 * 0.7).sin() / (1.0 + index as f32)).collect();
		let mut real_fft = RealFft::new(fft_size);
		let mut kernel = FftFrame::new(fft_size);
		real_fft.do_padded_fft(&impulse_response, &mut kernel);
		let input: Vec<f32> = (0..400).map(|index| ((index * 13 % 29) as f32 - 14.0) / 20.0).collect();
		let mut convolver = FftConvolver::new(fft_size);
		let mut output = input.clone();
		for chunk in output.chunks_mut(16) {
			convolver.process(&kernel, chunk);
		}
		for index in half_size..input.len() {
			let source_index = index - half_size;
			let expected: f32 = (0..half_size).filter(|tap| *tap <= source_index).map(|tap| impulse_response[tap] * input[source_index - tap]).sum();
			assert!((output[index] - expected).abs() < 1e-5, "{}: {} / {}", index, output[index], expected);
		}
	}
}
