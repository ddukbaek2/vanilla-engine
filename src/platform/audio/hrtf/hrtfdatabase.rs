//==============================================================================
//! HRTF 커널, 고도, 데이터베이스, 로더. (크로미엄 third_party/blink/renderer/platform/audio/hrtf_kernel.cc, hrtf_elevation.cc, hrtf_database.cc,
//! hrtf_database_loader.cc 와 audio_bus.cc 의 GetDataResource, TryCreateBySampleRateConverting 을 옮긴 것, Chromium Authors BSD-3-Clause)
//! - 자료: 크롬이 싣는 resources/Composite.flac 그대로다. (16 비트 스테레오 44100 Hz, 방위 24 x 높이 10 = 240 개 응답을 256 프레임씩 이은 61440 프레임,
//!   방위 0 ~ 345 를 15 도씩, 방위마다 높이 0, 15, 30, 45, 60, 75, 90, 315, 330, 345 차례) 크롬처럼 decodeAudioData 와 같은 해석기로 한 번 풀어 둔다.
//! - 커널(HRTFKernel): 응답을 컨텍스트 표본율로 바꾸고(decodeAudioData 와 같은 SincResampler), 분석 크기(FFT 크기 / 2)의 FFT 로 평균 군지연을 빼 그 값을
//!   귀 사이 지연으로 들고, FFT 크기 / 2 로 자른 끝 표본율 / 4410 프레임을 줄여 없앤 뒤 0 을 덧대 FFT 한다.
//! - 고도(HRTFElevation): 방위 24 개를 읽고(그 방위의 가장 높은 높이로 자른다) 사이를 8 배로 보간해 방위 192 개다. (커널은 섞은 틀, 지연은 lerp)
//! - 데이터베이스(HRTFDatabase): 높이 -45 ~ 90 을 15 도씩 10 개. 높이 보간 배수가 1 이라 높이는 15 도 칸으로 내림해 고른다.
//! - 로더(HRTFDatabaseLoader): 표본율마다 하나를 나눠 쓴다. 처음 부를 때 따로 스레드에서 짓고, 다 지을 때까지 데이터베이스가 없다.
//==============================================================================
use super::super::decode;
use super::fftframe::{FftFrame, RealFft};
use super::fft_size_for_sample_rate;
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// 크롬이 싣는 HRTF 자료. (IDR_AUDIO_SPATIALIZATION_COMPOSITE, resources/README.md)
const COMPOSITE_FLAC: &[u8] = include_bytes!("resources/Composite.flac");
/// 자료에서 읽는 방위 간격. (도, kAzimuthSpacing)
const AZIMUTH_SPACING: usize = 15;
/// 자료의 방위 수. (kNumberOfRawAzimuths)
const NUMBER_OF_RAW_AZIMUTHS: usize = 360 / AZIMUTH_SPACING;
/// 방위 보간 배수. (kInterpolationFactor)
const AZIMUTH_INTERPOLATION_FACTOR: usize = 8;
/// 보간한 뒤 방위 수. (kNumberOfTotalAzimuths)
pub(super) const NUMBER_OF_TOTAL_AZIMUTHS: usize = NUMBER_OF_RAW_AZIMUTHS * AZIMUTH_INTERPOLATION_FACTOR;
/// 응답 수. (kTotalNumberOfResponses)
const TOTAL_NUMBER_OF_RESPONSES: usize = 240;
/// 응답 하나의 프레임 수. (kResponseFrameSize)
const RESPONSE_FRAME_SIZE: usize = 256;
/// 자료의 표본율. (kResponseSampleRate)
const RESPONSE_SAMPLE_RATE: f32 = 44100.0;
/// 이은 자료 안 높이의 차례. (kElevationIndexTable, WebKit 버그 98294 c9)
const ELEVATION_INDEX_TABLE: [i32; 10] = [0, 15, 30, 45, 60, 75, 90, 315, 330, 345];
/// 방위마다 자료가 있는 가장 높은 높이. (kMaxElevations, 가장 낮은 높이는 모두 -45)
const MAXIMUM_ELEVATION_LIST: [i32; NUMBER_OF_RAW_AZIMUTHS] = [90, 45, 60, 45, 75, 45, 60, 45, 75, 45, 60, 45, 75, 45, 60, 45, 75, 45, 60, 45, 75, 45, 60, 45];
/// 데이터베이스의 가장 낮은 높이와 가장 높은 높이. (도, kMinElevation, kMaxElevation)
const MINIMUM_ELEVATION: i32 = -45;
const MAXIMUM_ELEVATION: i32 = 90;
/// 자료의 높이 간격. (도, kRawElevationAngleSpacing)
const RAW_ELEVATION_ANGLE_SPACING: i32 = 15;
/// 자료의 높이 수. (kNumberOfRawElevations, -45 ~ 90 의 10 개)
const NUMBER_OF_RAW_ELEVATIONS: usize = 1 + ((MAXIMUM_ELEVATION - MINIMUM_ELEVATION) / RAW_ELEVATION_ANGLE_SPACING) as usize;

//==============================================================================
/// 선형 보간. (libc++ std::lerp, f32 판. 한쪽이 0 을 넘나들면 t b + (1 - t) a, 아니면 a + t (b - a) 를 b 쪽으로 자른다)
///
/// # 인자
/// - `start`: t = 0 의 값.
/// - `end`: t = 1 의 값.
/// - `t`: 비.
///
/// # 돌려줌
/// 보간한 값.
//==============================================================================
pub(super) fn lerp_f32(start: f32, end: f32, t: f32) -> f32 {
	if (start <= 0.0 && end >= 0.0) || (start >= 0.0 && end <= 0.0) {
		return t * end + (1.0 - t) * start;
	}
	if t == 1.0 {
		return end;
	}
	let value = start + t * (end - start);
	if (t > 1.0) == (end > start) {
		if end < value {
			value
		}
		else {
			end
		}
	}
	else if value < end {
		value
	}
	else {
		end
	}
}

//==============================================================================
/// 선형 보간. (libc++ std::lerp, f64 판)
///
/// # 인자
/// - `start`: t = 0 의 값.
/// - `end`: t = 1 의 값.
/// - `t`: 비.
///
/// # 돌려줌
/// 보간한 값.
//==============================================================================
pub(super) fn lerp_f64(start: f64, end: f64, t: f64) -> f64 {
	if (start <= 0.0 && end >= 0.0) || (start >= 0.0 && end <= 0.0) {
		return t * end + (1.0 - t) * start;
	}
	if t == 1.0 {
		return end;
	}
	let value = start + t * (end - start);
	if (t > 1.0) == (end > start) {
		if end < value {
			value
		}
		else {
			end
		}
	}
	else if value < end {
		value
	}
	else {
		end
	}
}

//==============================================================================
/// 이은 응답 자료. (GetConcatenatedImpulseResponsesForSubject, 처음 부를 때 한 번 푼다. [왼쪽, 오른쪽])
///
/// # 돌려줌
/// 두 채널. (채널마다 61440 프레임)
//==============================================================================
fn get_concatenated_impulse_responses() -> &'static [Vec<f32>] {
	static CHANNEL_LIST: OnceLock<Vec<Vec<f32>>> = OnceLock::new();
	CHANNEL_LIST.get_or_init(|| {
		let channel_list = decode::decode_audio_channels(COMPOSITE_FLAC).expect("Composite.flac");
		assert!(channel_list.len() == 2 && channel_list[0].len() == TOTAL_NUMBER_OF_RESPONSES * RESPONSE_FRAME_SIZE, "Composite.flac");
		channel_list
	})
}

//==============================================================================
/// HRTF 커널 하나. (HRTFKernel: 0 을 덧댄 응답의 주파수 틀과 귀 사이 지연)
//==============================================================================
pub(super) struct HrtfKernel {
	fft_frame: FftFrame,
	/// 응답에서 뺀 지연. (프레임)
	frame_delay: f32,
}

impl HrtfKernel {
	//==============================================================================
	/// 응답으로 짓기. (HRTFKernel 생성자)
	///
	/// # 인자
	/// - `impulse_response`: 응답. (분석 크기 앞쪽이 군지연을 뺀 값으로 바뀐다)
	/// - `fft_size`: FFT 크기.
	/// - `sample_rate`: 표본율.
	/// - `analysis_fft`: FFT 크기 / 2 의 FFT.
	/// - `real_fft`: FFT 크기의 FFT.
	///
	/// # 돌려줌
	/// 커널.
	//==============================================================================
	fn new(impulse_response: &mut [f32], fft_size: usize, sample_rate: f32, analysis_fft: &mut RealFft, real_fft: &mut RealFft) -> HrtfKernel {
		// 앞머리 지연. (평균 군지연)
		let frame_delay = extract_average_group_delay(impulse_response, fft_size / 2, analysis_fft);
		// FFT 크기 / 2 로 자른다. (0 을 덧대야 합성곱이 맞다)
		let truncated_response_length = impulse_response.len().min(fft_size / 2);
		// 자른 자리를 빠르게 줄여 없앤다. (44100 Hz 에서 10 프레임)
		let number_of_fade_out_frames = (sample_rate / 4410.0) as usize;
		for index in truncated_response_length - number_of_fade_out_frames..truncated_response_length {
			let x = 1.0f32 - (index - (truncated_response_length - number_of_fade_out_frames)) as f32 / number_of_fade_out_frames as f32;
			impulse_response[index] *= x;
		}
		let mut fft_frame = FftFrame::new(fft_size);
		real_fft.do_padded_fft(&impulse_response[..truncated_response_length], &mut fft_frame);
		HrtfKernel { fft_frame, frame_delay }
	}

	//==============================================================================
	/// 두 커널 사이를 섞은 커널. (HRTFKernel::CreateInterpolatedKernel, 지연은 f32 lerp, 틀은 CreateInterpolatedFrame)
	///
	/// # 인자
	/// - `kernel1`: x = 0 쪽.
	/// - `kernel2`: x = 1 쪽.
	/// - `x`: 비. (0 ~ 1)
	/// - `real_fft`: FFT 크기의 FFT.
	///
	/// # 돌려줌
	/// 섞은 커널.
	//==============================================================================
	fn create_interpolated_kernel(kernel1: &HrtfKernel, kernel2: &HrtfKernel, x: f32, real_fft: &mut RealFft) -> HrtfKernel {
		let x = x.clamp(0.0, 1.0);
		let frame_delay = lerp_f32(kernel1.frame_delay, kernel2.frame_delay, x);
		let fft_frame = FftFrame::create_interpolated_frame(&kernel1.fft_frame, &kernel2.fft_frame, x as f64, real_fft);
		HrtfKernel { fft_frame, frame_delay }
	}

	//==============================================================================
	/// 주파수 틀.
	///
	/// # 돌려줌
	/// 틀.
	//==============================================================================
	pub(super) fn get_fft_frame(&self) -> &FftFrame {
		&self.fft_frame
	}
}

//==============================================================================
/// 평균 군지연을 빼고 그 값을 돌려준다. (hrtf_kernel.cc ExtractAverageGroupDelay, 응답 앞쪽 분석 크기만 FFT 하고 되돌려 쓴다)
///
/// # 인자
/// - `impulse_response`: 응답.
/// - `analysis_fft_size`: 분석 크기. (2 의 거듭제곱)
/// - `analysis_fft`: 분석 크기의 FFT.
///
/// # 돌려줌
/// 뺀 지연. (프레임, f32 범위로 자른다)
//==============================================================================
fn extract_average_group_delay(impulse_response: &mut [f32], analysis_fft_size: usize, analysis_fft: &mut RealFft) -> f32 {
	let impulse_span = &mut impulse_response[..analysis_fft_size];
	let mut estimation_frame = FftFrame::new(analysis_fft_size);
	analysis_fft.do_fft(impulse_span, &mut estimation_frame);
	let frame_delay = estimation_frame.extract_average_group_delay().clamp(f32::MIN as f64, f32::MAX as f64) as f32;
	analysis_fft.do_inverse_fft(&estimation_frame, impulse_span);
	frame_delay
}

//==============================================================================
/// 방위와 높이 하나의 왼쪽, 오른쪽 커널. (CalculateKernelsForAzimuthElevation, 방위 0 ~ 345 와 높이 -45 ~ 90 을 15 도씩)
///
/// # 인자
/// - `azimuth`: 방위. (도)
/// - `elevation`: 높이. (도)
/// - `sample_rate`: 표본율.
/// - `analysis_fft`: FFT 크기 / 2 의 FFT.
/// - `real_fft`: FFT 크기의 FFT.
///
/// # 돌려줌
/// (왼쪽 커널, 오른쪽 커널).
//==============================================================================
fn calculate_kernels_for_azimuth_elevation(azimuth: usize, elevation: i32, sample_rate: f32, analysis_fft: &mut RealFft, real_fft: &mut RealFft) -> (HrtfKernel, HrtfKernel) {
	let positive_elevation = if elevation < 0 { elevation + 360 } else { elevation };
	let channel_list = get_concatenated_impulse_responses();
	let elevation_index = ELEVATION_INDEX_TABLE.iter().position(|table_elevation| *table_elevation == positive_elevation).expect("elevation");
	// 이은 자료는 방위마다 높이를 차례로 둔다.
	let index = (azimuth / AZIMUTH_SPACING) * NUMBER_OF_RAW_ELEVATIONS + elevation_index;
	let start_frame = index * RESPONSE_FRAME_SIZE;
	let stop_frame = start_frame + RESPONSE_FRAME_SIZE;
	let kernel_left = create_kernel_from_response(&channel_list[0][start_frame..stop_frame], sample_rate, analysis_fft, real_fft);
	let kernel_right = create_kernel_from_response(&channel_list[1][start_frame..stop_frame], sample_rate, analysis_fft, real_fft);
	(kernel_left, kernel_right)
}

//==============================================================================
/// 자료의 응답 하나로 커널 짓기. (표본율을 바꾸고, FFT 크기 / 2 보다 짧으면 0 을 덧대 HRTFKernel 을 짓는다)
///
/// # 인자
/// - `response`: 44100 Hz 응답. (256 프레임)
/// - `sample_rate`: 표본율.
/// - `analysis_fft`: FFT 크기 / 2 의 FFT.
/// - `real_fft`: FFT 크기의 FFT.
///
/// # 돌려줌
/// 커널.
//==============================================================================
fn create_kernel_from_response(response: &[f32], sample_rate: f32, analysis_fft: &mut RealFft, real_fft: &mut RealFft) -> HrtfKernel {
	let fft_size = fft_size_for_sample_rate(sample_rate);
	let mut impulse_response = decode::resample::resample_channel(response, RESPONSE_SAMPLE_RATE as f64, sample_rate as f64);
	if 2 * impulse_response.len() < fft_size {
		impulse_response.resize(fft_size / 2, 0.0);
	}
	HrtfKernel::new(&mut impulse_response, fft_size, sample_rate, analysis_fft, real_fft)
}

//==============================================================================
/// 높이 하나의 방위 192 개 커널. (HRTFElevation)
//==============================================================================
struct HrtfElevation {
	kernel_list_left: Vec<HrtfKernel>,
	kernel_list_right: Vec<HrtfKernel>,
}

impl HrtfElevation {
	//==============================================================================
	/// 높이 하나 짓기. (HRTFElevation::CreateForSubject, 방위 24 개를 읽고 사이를 8 배로 보간한다)
	///
	/// # 인자
	/// - `elevation`: 높이. (도, -45 ~ 90 을 15 도씩)
	/// - `sample_rate`: 표본율.
	/// - `analysis_fft`: FFT 크기 / 2 의 FFT.
	/// - `real_fft`: FFT 크기의 FFT.
	///
	/// # 돌려줌
	/// 높이.
	//==============================================================================
	fn create(elevation: i32, sample_rate: f32, analysis_fft: &mut RealFft, real_fft: &mut RealFft) -> HrtfElevation {
		let mut raw_kernel_list = Vec::with_capacity(NUMBER_OF_RAW_AZIMUTHS);
		for (raw_index, maximum_elevation) in MAXIMUM_ELEVATION_LIST.iter().enumerate() {
			// 그 방위에 자료가 있는 높이까지만.
			let actual_elevation = elevation.min(*maximum_elevation);
			raw_kernel_list.push(calculate_kernels_for_azimuth_elevation(raw_index * AZIMUTH_SPACING, actual_elevation, sample_rate, analysis_fft, real_fft));
		}
		// 사이 방위를 보간한다. (마지막은 첫 방위와)
		let mut kernel_list_left = Vec::with_capacity(NUMBER_OF_TOTAL_AZIMUTHS);
		let mut kernel_list_right = Vec::with_capacity(NUMBER_OF_TOTAL_AZIMUTHS);
		let mut interpolated_list = Vec::with_capacity(NUMBER_OF_RAW_AZIMUTHS);
		for raw_index in 0..NUMBER_OF_RAW_AZIMUTHS {
			let next_index = (raw_index + 1) % NUMBER_OF_RAW_AZIMUTHS;
			let (left1, right1) = &raw_kernel_list[raw_index];
			let (left2, right2) = &raw_kernel_list[next_index];
			let mut kernel_pair_list = Vec::with_capacity(AZIMUTH_INTERPOLATION_FACTOR - 1);
			for step in 1..AZIMUTH_INTERPOLATION_FACTOR {
				let x = step as f32 / AZIMUTH_INTERPOLATION_FACTOR as f32;
				let left = HrtfKernel::create_interpolated_kernel(left1, left2, x, real_fft);
				let right = HrtfKernel::create_interpolated_kernel(right1, right2, x, real_fft);
				kernel_pair_list.push((left, right));
			}
			interpolated_list.push(kernel_pair_list);
		}
		for ((left, right), kernel_pair_list) in raw_kernel_list.into_iter().zip(interpolated_list.into_iter()) {
			kernel_list_left.push(left);
			kernel_list_right.push(right);
			for (interpolated_left, interpolated_right) in kernel_pair_list {
				kernel_list_left.push(interpolated_left);
				kernel_list_right.push(interpolated_right);
			}
		}
		HrtfElevation { kernel_list_left, kernel_list_right }
	}

	//==============================================================================
	/// 방위의 커널과 지연. (HRTFElevation::GetKernelsFromAzimuth, 지연은 다음 방위와 azimuth_blend 로 lerp)
	///
	/// # 인자
	/// - `azimuth_blend`: 다음 방위 쪽 비. (0 ~ 1)
	/// - `azimuth_index`: 방위 번호. (0 ~ 191)
	///
	/// # 돌려줌
	/// (왼쪽 커널, 오른쪽 커널, 왼쪽 지연, 오른쪽 지연).
	//==============================================================================
	fn get_kernels_from_azimuth(&self, azimuth_blend: f64, azimuth_index: usize) -> (&HrtfKernel, &HrtfKernel, f64, f64) {
		let kernel_left = &self.kernel_list_left[azimuth_index];
		let kernel_right = &self.kernel_list_right[azimuth_index];
		let next_index = (azimuth_index + 1) % self.kernel_list_left.len();
		let frame_delay_left = lerp_f64(kernel_left.frame_delay as f64, self.kernel_list_left[next_index].frame_delay as f64, azimuth_blend);
		let frame_delay_right = lerp_f64(kernel_right.frame_delay as f64, self.kernel_list_right[next_index].frame_delay as f64, azimuth_blend);
		(kernel_left, kernel_right, frame_delay_left, frame_delay_right)
	}
}

//==============================================================================
/// HRTF 데이터베이스. (HRTFDatabase, 높이 10 개)
//==============================================================================
pub(super) struct HrtfDatabase {
	elevation_list: Vec<HrtfElevation>,
}

impl HrtfDatabase {
	//==============================================================================
	/// 표본율의 데이터베이스 짓기. (HRTFDatabase 생성자, 높이 보간 배수가 1 이라 사이 높이는 짓지 않는다)
	///
	/// # 인자
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 데이터베이스.
	//==============================================================================
	fn new(sample_rate: f32) -> HrtfDatabase {
		let fft_size = fft_size_for_sample_rate(sample_rate);
		let mut analysis_fft = RealFft::new(fft_size / 2);
		let mut real_fft = RealFft::new(fft_size);
		let elevation_list = (MINIMUM_ELEVATION..=MAXIMUM_ELEVATION)
			.step_by(RAW_ELEVATION_ANGLE_SPACING as usize)
			.map(|elevation| HrtfElevation::create(elevation, sample_rate, &mut analysis_fft, &mut real_fft))
			.collect();
		HrtfDatabase { elevation_list }
	}

	//==============================================================================
	/// 방위와 높이의 커널과 지연. (HRTFDatabase::GetKernelsFromAzimuthElevation, 높이는 -45 ~ 90 으로 자르고 15 도 칸으로 내림)
	///
	/// # 인자
	/// - `azimuth_blend`: 다음 방위 쪽 비.
	/// - `azimuth_index`: 방위 번호.
	/// - `elevation_angle`: 높이. (도)
	///
	/// # 돌려줌
	/// (왼쪽 커널, 오른쪽 커널, 왼쪽 지연, 오른쪽 지연).
	//==============================================================================
	pub(super) fn get_kernels_from_azimuth_elevation(&self, azimuth_blend: f64, azimuth_index: usize, elevation_angle: f64) -> (&HrtfKernel, &HrtfKernel, f64, f64) {
		let clamped_elevation = super::super::pannermath::clamp_to(elevation_angle, MINIMUM_ELEVATION as f64, MAXIMUM_ELEVATION as f64);
		let elevation_index = ((clamped_elevation - MINIMUM_ELEVATION as f64) / RAW_ELEVATION_ANGLE_SPACING as f64) as i32 as usize;
		let elevation_index = elevation_index.min(self.elevation_list.len() - 1);
		self.elevation_list[elevation_index].get_kernels_from_azimuth(azimuth_blend, azimuth_index)
	}
}

//==============================================================================
/// 표본율 하나의 데이터베이스 로더. (HRTFDatabaseLoader, 따로 스레드에서 짓는다)
//==============================================================================
pub(crate) struct HrtfDatabaseLoader {
	database_sample_rate: f32,
	database: OnceLock<HrtfDatabase>,
	thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

impl HrtfDatabaseLoader {
	//==============================================================================
	/// 표본율의 로더. (CreateAndLoadAsynchronouslyIfNecessary, 이미 있으면 그것, 없으면 짓기 시작한 새 로더)
	///
	/// # 인자
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 로더.
	//==============================================================================
	pub(crate) fn create_and_load_asynchronously_if_necessary(sample_rate: f32) -> Arc<HrtfDatabaseLoader> {
		// 크롬의 LoaderMap 처럼 표본율마다 하나를 약한 참조로 든다.
		static LOADER_LIST: Mutex<Vec<Weak<HrtfDatabaseLoader>>> = Mutex::new(Vec::new());
		let mut loader_list = LOADER_LIST.lock().unwrap();
		loader_list.retain(|loader| loader.strong_count() > 0);
		if let Some(loader) = loader_list.iter().filter_map(|loader| loader.upgrade()).find(|loader| loader.database_sample_rate == sample_rate) {
			return loader;
		}
		let loader = Arc::new(HrtfDatabaseLoader {
			database_sample_rate: sample_rate,
			database: OnceLock::new(),
			thread: Mutex::new(None),
		});
		loader_list.push(Arc::downgrade(&loader));
		loader.load_asynchronously();
		loader
	}

	//==============================================================================
	/// 따로 스레드에서 짓기 시작. (LoadAsynchronously, LoadTask)
	//==============================================================================
	fn load_asynchronously(self: &Arc<HrtfDatabaseLoader>) {
		let loader = self.clone();
		let handle = std::thread::Builder::new()
			.name(String::from("HRTFDatabaseLoader"))
			.spawn(move || {
				let _ = loader.database.set(HrtfDatabase::new(loader.database_sample_rate));
			})
			.expect("HRTFDatabaseLoader");
		*self.thread.lock().unwrap() = Some(handle);
	}

	//==============================================================================
	/// 데이터베이스. (Database, 다 짓기 전에는 None)
	///
	/// # 돌려줌
	/// 데이터베이스.
	//==============================================================================
	pub(super) fn get_database(&self) -> Option<&HrtfDatabase> {
		self.database.get()
	}

	//==============================================================================
	/// 표본율.
	///
	/// # 돌려줌
	/// 짓는 데이터베이스의 표본율.
	//==============================================================================
	pub(crate) fn get_sample_rate(&self) -> f32 {
		self.database_sample_rate
	}

	//==============================================================================
	/// 짓지 않는 로더. (시험, 데이터베이스가 없는 동안의 길)
	///
	/// # 인자
	/// - `sample_rate`: 표본율.
	///
	/// # 돌려줌
	/// 로더.
	//==============================================================================
	#[cfg(test)]
	pub(super) fn create_unloaded(sample_rate: f32) -> Arc<HrtfDatabaseLoader> {
		Arc::new(HrtfDatabaseLoader {
			database_sample_rate: sample_rate,
			database: OnceLock::new(),
			thread: Mutex::new(None),
		})
	}

	//==============================================================================
	/// 짓기가 끝날 때까지 기다린다. (WaitForLoaderThreadCompletion, 크롬 OfflineAudioContext 의 길, 시험이 쓴다)
	//==============================================================================
	#[cfg(test)]
	pub(crate) fn wait_for_loader_thread_completion(&self) {
		if let Some(handle) = self.thread.lock().unwrap().take() {
			let _ = handle.join();
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	/// 자료는 두 채널 61440 프레임으로 풀리고, 앞 응답(방위 0, 높이 0)은 0 이 아니다.
	//==============================================================================
	#[test]
	fn composite_decodes_like_chrome() {
		let channel_list = get_concatenated_impulse_responses();
		assert_eq!(channel_list.len(), 2);
		assert_eq!(channel_list[0].len(), 61440);
		assert!(channel_list[0][..256].iter().any(|sample| *sample != 0.0));
		// 16 비트 정수를 크롬처럼 실수로 바꿨다. (음수는 32768, 양수는 32767 로 나눈 값)
		assert!(channel_list.iter().flatten().all(|sample| (-1.0..=1.0).contains(sample)));
	}

	//==============================================================================
	/// libc++ std::lerp 의 갈래. (0 을 넘나듦, 끝, 단조)
	//==============================================================================
	#[test]
	fn lerp_follows_libcxx() {
		assert_eq!(lerp_f64(0.0, 10.0, 0.25), 2.5);
		assert_eq!(lerp_f64(2.0, 4.0, 1.0), 4.0);
		assert_eq!(lerp_f64(2.0, 4.0, 0.5), 3.0);
		assert_eq!(lerp_f32(-1.0, 3.0, 0.5), 1.0);
		assert_eq!(lerp_f32(5.0, 1.0, 0.75), 2.0);
	}
}
