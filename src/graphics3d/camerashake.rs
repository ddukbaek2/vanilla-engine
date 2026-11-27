//==============================================================================
//! 3D 카메라 흔들림. (vanilla.js graphics3d/camerashake.js 의 CameraShake 와 createOffsetViewMatrix)
//! - 유니티 Cinemachine Impulse, 언리얼 Camera Shake 꼴의 엔진 흔들림이다. 흔들림마다 자리 3 축과 돌림 3 축(pitch, yaw, roll)의 결, 세기, 주파수, 길이, 오름, 잦아듦을 따로 쥔다.
//! - 결: 축마다 주파수가 조금씩 다른 사인파 둘을 겹친다. 위상과 주파수 몫은 흔들림의 씨앗에서 뽑는다. (같은 씨앗, 같은 나이면 같은 값)
//! - 크기 곡선: 나이가 음수면 아직 0(그만큼 늦게 시작), attack 초 동안 0 에서 1 로 오르고, (1 - 나이 / 길이) 의 decay_exponent 제곱으로 잦아들어 길이에서 0.
//! - 겹침: 사는 흔들림을 모두 더한다. (센 쪽을 따르는 2D Shaker 와 다르다)
//! - 나이: CameraShakeOptions 의 age 는 이미 지난 초다. 결도 나이로 셈해 늦게 보인 사건을 그 나이로 넣으면 처음부터 넣은 것과 같은 값이 이어진다.
//! - 거는 곳: Camera3D::set_view_offset(자리, 돌림) 에 넣거나, 게임이 셈한 보기 행렬이면 create_offset_view_matrix 로 건다.
//! - 원본과 같은 씨앗이면 같은 값이 나온다. 씨앗을 주지 않은 흔들림은 바탕 SeededRandom 의 다음 값 x 2^32 을 내린 수를 씨앗으로 쓰고,
//!   파 값 12 개(축 여섯마다 파 둘, 파마다 주파수 몫 다음 위상)를 그 씨앗의 SeededRandom 에서 원본과 같은 차례로 뽑는다.
//!   사인은 크롬 Math.sin 과 같은 비트의 엔진 사인(base 의 바르게 반올림한 sin)이다.
//! - 원본과 다른 것: 원본 생성자의 { seed } 는 CameraShake::new(seed) 이고, add 의 칸 객체는 CameraShakeOptions(Default 가 원본 기본 값)다.
//!   원본 read 의 { position, rotation } 은 CameraShakeOffset 이다. 씨앗은 원본 SeededRandom 처럼 u32 다.
//==============================================================================
use crate::base::math;
use crate::base::matrix4::Matrix4;
use crate::base::seededrandom::SeededRandom;
use crate::base::trigonometry;
use crate::base::vector3::Vector3;

/// 한 축의 흔들림 결을 이루는 사인파 수. (Cinemachine 잡음 설정처럼 주파수가 다른 파를 겹쳐 궤적이 되풀이되지 않게)
const WAVES_PER_CHANNEL: usize = 2;
/// 흔들림 결의 축 수. (자리 x, y, z 와 돌림 pitch, yaw, roll)
const CHANNEL_COUNT: usize = 6;
/// 사인파마다의 세기. (합이 1 이 되게 나눠 결이 -1 ~ 1 안에 든다)
const WAVE_WEIGHTS: [f64; WAVES_PER_CHANNEL] = [2.0 / 3.0, 1.0 / 3.0];
/// 사인파마다의 주파수 몫 범위. ([아래, 위], 흔들림 주파수에 곱한다)
const WAVE_RATIO_RANGES: [[f64; 2]; WAVES_PER_CHANNEL] = [[0.85, 1.15], [1.7, 2.3]];
/// 씨앗을 주지 않은 흔들림의 씨앗을 뽑을 때 난수에 곱하는 수. (2^32, 원본 floor(nextValue() x 4294967296))
const SEED_RANGE: f64 = 4294967296.0;

//==============================================================================
/// 보기 행렬에 카메라 지역 덧붙임 걸기. (카메라를 그 자리에서 position 만큼 옮기고 rotation 만큼 돌린 보기, 새 행렬)
/// - position 은 카메라 지역 축이다. (x 오른쪽, y 위, z 뒤, 카메라는 -Z 를 봄)
/// - rotation 은 카메라 지역 축의 각(라디안)이다. (x 는 pitch, y 는 yaw, z 는 roll, roll 다음 pitch 다음 yaw 로 건다)
/// - 덧붙인 카메라는 (카메라) x 옮김 x yaw x pitch x roll 이고, 보기 행렬은 그 역이라 (옮김 x yaw x pitch x roll) 의 역을 앞에 곱한다.
///
/// # 인자
/// - `view_matrix`: 덧붙이기 앞의 보기 행렬. (월드 → 카메라, 고치지 않는다)
/// - `position`: 카메라 지역 자리 덧붙임. (장면 길이)
/// - `rotation`: 카메라 지역 돌림 덧붙임. (라디안, x pitch, y yaw, z roll)
///
/// # 돌려줌
/// 덧붙인 보기 행렬. (새 행렬)
///
/// # 예제
/// 보기 행렬을 오른쪽으로 0.1, 고개를 0.02 라디안 들어 덧붙이기
/// ```rust
/// use vanilla::{create_offset_view_matrix, Matrix4, Vector3};
///
/// let view_matrix = Matrix4::create_look_at(Vector3::create(0.0, 2.0, 6.0), Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
/// let shaken_view_matrix = create_offset_view_matrix(&view_matrix, Vector3::create(0.1, 0.0, 0.0), Vector3::create(0.02, 0.0, 0.0));
/// assert_eq!(shaken_view_matrix.get_elements().len(), 16);
/// assert_eq!(create_offset_view_matrix(&view_matrix, Vector3::zero(), Vector3::zero()).get_elements(), view_matrix.get_elements());
/// ```
//==============================================================================
pub fn create_offset_view_matrix(view_matrix: &Matrix4, position: Vector3, rotation: Vector3) -> Matrix4 {
	let mut offset_matrix = Matrix4::create_rotation_z(-rotation.z);
	offset_matrix.rotate_x(-rotation.x);
	offset_matrix.rotate_y(-rotation.y);
	offset_matrix.translate(-position.x, -position.y, -position.z);
	offset_matrix.multiply(view_matrix);
	offset_matrix
}

//==============================================================================
/// 흔들림 하나의 칸. (CameraShake::add 에 준다, 원본 add 의 options, Default 가 원본 기본 값)
///
/// # 예제
/// 위아래로 덜 흔들리고 조금 늦게 시작하는 흔들림
/// ```rust
/// use vanilla::{CameraShake, CameraShakeOptions};
///
/// let mut camera_shake = CameraShake::new(1);
/// camera_shake.add(CameraShakeOptions { amplitude: 0.05, position_axes: [1.0, 0.3, 1.0], duration: 0.4, age: -0.1, ..Default::default() });
/// assert!(camera_shake.is_shaking());
/// assert_eq!(camera_shake.read().position.x, 0.0);
/// ```
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CameraShakeOptions {
	/// 자리 흔들림의 최대 길이. (장면 길이, 기본 0.1)
	pub amplitude: f64,
	/// 돌림 흔들림의 최대 각. (라디안, 기본 0)
	pub rotation_amplitude: f64,
	/// 자리 축마다의 몫. ([x, y, z] 카메라 지역 축 오른쪽, 위, 뒤, 기본 [1, 1, 1])
	pub position_axes: [f64; 3],
	/// 돌림 축마다의 몫. ([pitch, yaw, roll], 기본 [1, 1, 1])
	pub rotation_axes: [f64; 3],
	/// 주파수. (초당 번, 기본 10)
	pub frequency: f64,
	/// 길이. (초, 기본 0.25)
	pub duration: f64,
	/// 0 에서 다 센 데까지 오르는 초. (기본 0)
	pub attack: f64,
	/// 잦아듦 지수. ((1 - 나이 / 길이) 의 이 제곱, 1 이면 곧게, 0 이면 끝까지 그대로, 기본 2)
	pub decay_exponent: f64,
	/// 이미 지난 초. (기본 0, 음수면 그만큼 늦게 시작)
	pub age: f64,
	/// 이 흔들림의 씨앗. (None 이면 바탕 씨앗에서 차례로 뽑음)
	pub seed: Option<u32>,
}

impl Default for CameraShakeOptions {
	//==============================================================================
	/// 원본 기본 값. (세기 0.1, 돌림 0, 축 몫 1, 주파수 10, 길이 0.25, 오름 0, 잦아듦 지수 2, 나이 0, 씨앗 없음)
	//==============================================================================
	fn default() -> Self {
		CameraShakeOptions { amplitude: 0.1, rotation_amplitude: 0.0, position_axes: [1.0; 3], rotation_axes: [1.0; 3], frequency: 10.0, duration: 0.25, attack: 0.0, decay_exponent: 2.0, age: 0.0, seed: None }
	}
}

//==============================================================================
/// 지금의 흔들림 덧붙임. (CameraShake::read, 카메라 지역 축, 흔들림이 없으면 둘 다 영 벡터)
///
/// # 예제
/// 흔들림이 없으면 영 벡터
/// ```rust
/// use vanilla::{CameraShake, CameraShakeOffset, Vector3};
///
/// let camera_shake = CameraShake::new(1);
/// let offset: CameraShakeOffset = camera_shake.read();
/// assert_eq!(offset.position, Vector3::zero());
/// assert_eq!(offset.rotation, Vector3::zero());
/// ```
//==============================================================================
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct CameraShakeOffset {
	/// 자리. (장면 길이, x 오른쪽, y 위, z 뒤)
	pub position: Vector3,
	/// 돌림. (라디안, x pitch, y yaw, z roll)
	pub rotation: Vector3,
}

//==============================================================================
/// 사인파 하나. (주파수 몫과 위상)
//==============================================================================
#[derive(Clone, Copy, Debug)]
struct ShakeWave {
	ratio: f64,
	phase: f64,
}

//==============================================================================
/// 사는 흔들림 하나. (받은 칸과 나이, 축마다의 파)
//==============================================================================
#[derive(Clone, Debug)]
struct ActiveShake {
	options: CameraShakeOptions,
	age: f64,
	waves: [ShakeWave; CHANNEL_COUNT * WAVES_PER_CHANNEL],
}

//==============================================================================
/// 3D 카메라 흔들림. (원본 CameraShake, 흔들림을 더하고 나이를 먹여 지금의 카메라 지역 덧붙임을 읽는다)
/// - add 로 흔들림 하나를 더하고, update(초) 로 나이를 먹이고 다 된 흔들림을 지운다. read() 가 지금의 덧붙임을 준다.
/// - 기본(Default)은 바탕 씨앗 1 이다. (원본 생성자 기본)
///
/// # 예제
/// 폭발 흔들림을 Camera3D 에 걸기
/// ```rust
/// use vanilla::{Camera3D, CameraShake, CameraShakeOptions, Vector3};
///
/// let mut camera = Camera3D::new();
/// camera.set_perspective(std::f64::consts::PI / 3.0, 0.1, 500.0);
/// camera.look_at(Vector3::create(0.0, 4.0, 10.0), Vector3::zero(), Vector3::up());
/// let mut camera_shake = CameraShake::new(7);
/// camera_shake.add(CameraShakeOptions { amplitude: 0.09, rotation_amplitude: 0.01, duration: 0.25, position_axes: [1.0, 0.5, 1.0], ..Default::default() });
/// camera_shake.add(CameraShakeOptions { amplitude: 0.04, duration: 0.6, age: 0.1, ..Default::default() });
/// camera_shake.update(1.0 / 60.0);
/// let shake = camera_shake.read();
/// camera.set_view_offset(shake.position, shake.rotation);
/// let view_projection_matrix = camera.get_view_projection_matrix();
/// assert!(camera_shake.is_shaking());
/// assert_eq!(view_projection_matrix.get_elements().len(), 16);
/// ```
//==============================================================================
#[derive(Clone, Debug)]
pub struct CameraShake {
	/// 씨앗을 주지 않은 흔들림의 씨앗을 뽑는 난수.
	random: SeededRandom,
	/// 사는 흔들림. (더한 차례)
	shakes: Vec<ActiveShake>,
}

impl Default for CameraShake {
	//==============================================================================
	/// 바탕 씨앗 1 의 흔들림. (원본 new CameraShake() 와 같다)
	//==============================================================================
	fn default() -> Self {
		CameraShake::new(1)
	}
}

impl CameraShake {
	//==============================================================================
	/// 생성.
	///
	/// # 인자
	/// - `seed`: 씨앗을 주지 않은 흔들림의 씨앗을 뽑을 바탕 씨앗. (원본 { seed } 기본 1, 0 이면 SeededRandom 처럼 1)
	///
	/// # 돌려줌
	/// 흔들림이 없는 새 흔들림 바탕.
	///
	/// # 예제
	/// 같은 바탕 씨앗이면 같은 값
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut first = CameraShake::new(42);
	/// let mut second = CameraShake::new(42);
	/// first.add(CameraShakeOptions::default());
	/// second.add(CameraShakeOptions::default());
	/// first.update(0.05);
	/// second.update(0.05);
	/// assert_eq!(first.read(), second.read());
	/// ```
	//==============================================================================
	pub fn new(seed: u32) -> CameraShake {
		CameraShake { random: SeededRandom::new(seed), shakes: Vec::new() }
	}

	//==============================================================================
	/// 흔들림 더하기. (사는 흔들림에 겹쳐 더함)
	///
	/// # 인자
	/// - `options`: 흔들림 칸. (세기, 돌림 세기, 축 몫, 주파수, 길이, 오름, 잦아듦 지수, 나이, 씨앗)
	///
	/// # 예제
	/// 돌림만 흔드는 흔들림
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut camera_shake = CameraShake::new(3);
	/// camera_shake.add(CameraShakeOptions { amplitude: 0.0, rotation_amplitude: 0.05, rotation_axes: [1.0, 1.0, 0.0], seed: Some(9), ..Default::default() });
	/// camera_shake.update(0.02);
	/// let offset = camera_shake.read();
	/// assert_eq!(offset.position.x, 0.0);
	/// assert_eq!(offset.rotation.z, 0.0);
	/// assert!(offset.rotation.x != 0.0);
	/// ```
	//==============================================================================
	pub fn add(&mut self, options: CameraShakeOptions) {
		let seed = match options.seed {
			Some(seed) => {
				seed
			}
			None => {
				math::floor(self.random.next_value() * SEED_RANGE) as u32
			}
		};
		let mut wave_random = SeededRandom::new(seed);
		let mut waves = [ShakeWave { ratio: 0.0, phase: 0.0 }; CHANNEL_COUNT * WAVES_PER_CHANNEL];
		for channel_index in 0..CHANNEL_COUNT {
			for wave_index in 0..WAVES_PER_CHANNEL {
				let ratio_range = WAVE_RATIO_RANGES[wave_index];
				let ratio = wave_random.next_range(ratio_range[0], ratio_range[1]);
				let phase = wave_random.next_range(0.0, std::f64::consts::PI * 2.0);
				waves[channel_index * WAVES_PER_CHANNEL + wave_index] = ShakeWave { ratio, phase };
			}
		}
		self.shakes.push(ActiveShake { options, age: options.age, waves });
	}

	//==============================================================================
	/// 나이 먹이기. (다 된 흔들림을 지움)
	///
	/// # 인자
	/// - `seconds`: 지난 프레임부터의 시간. (초)
	///
	/// # 예제
	/// 길이가 지나면 흔들림이 끝난다
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut camera_shake = CameraShake::new(1);
	/// camera_shake.add(CameraShakeOptions { duration: 0.2, ..Default::default() });
	/// camera_shake.update(0.1);
	/// assert!(camera_shake.is_shaking());
	/// camera_shake.update(0.1);
	/// assert!(!camera_shake.is_shaking());
	/// ```
	//==============================================================================
	pub fn update(&mut self, seconds: f64) {
		self.shakes.retain_mut(|shake| {
			shake.age += seconds;
			shake.age < shake.options.duration
		});
	}

	//==============================================================================
	/// 지금의 덧붙임. (사는 흔들림을 모두 더함, 카메라 지역 축)
	///
	/// # 돌려줌
	/// 자리(장면 길이, x 오른쪽, y 위, z 뒤)와 돌림(라디안, x pitch, y yaw, z roll). (흔들림이 없으면 둘 다 영 벡터)
	///
	/// # 예제
	/// 자리 덧붙임은 세기 x 축 몫 안에 든다
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut camera_shake = CameraShake::new(5);
	/// camera_shake.add(CameraShakeOptions { amplitude: 0.2, duration: 1.0, ..Default::default() });
	/// camera_shake.update(0.1);
	/// let offset = camera_shake.read();
	/// assert!(offset.position.x.abs() <= 0.2 && offset.position.y.abs() <= 0.2 && offset.position.z.abs() <= 0.2);
	/// ```
	//==============================================================================
	pub fn read(&self) -> CameraShakeOffset {
		let mut offset = [0.0f64; CHANNEL_COUNT];
		for shake in self.shakes.iter() {
			let options = &shake.options;
			let age = shake.age;
			if age < 0.0 || age >= options.duration {
				continue;
			}
			let mut envelope = math::pow(1.0 - age / options.duration, options.decay_exponent);
			if options.attack > 0.0 && age < options.attack {
				envelope *= age / options.attack;
			}
			let angular_frequency = std::f64::consts::PI * 2.0 * options.frequency;
			for channel_index in 0..CHANNEL_COUNT {
				let is_position = channel_index < 3;
				let axis_index = channel_index % 3;
				let gain = if is_position { options.amplitude * options.position_axes[axis_index] } else { options.rotation_amplitude * options.rotation_axes[axis_index] };
				if gain == 0.0 {
					continue;
				}
				let mut signal = 0.0;
				for wave_index in 0..WAVES_PER_CHANNEL {
					let wave = shake.waves[channel_index * WAVES_PER_CHANNEL + wave_index];
					signal += trigonometry::sin(angular_frequency * wave.ratio * age + wave.phase) * WAVE_WEIGHTS[wave_index];
				}
				offset[channel_index] += signal * gain * envelope;
			}
		}
		CameraShakeOffset { position: Vector3::create(offset[0], offset[1], offset[2]), rotation: Vector3::create(offset[3], offset[4], offset[5]) }
	}

	//==============================================================================
	/// 흔들림이 남았는지. (늦게 시작할 흔들림도 셈)
	///
	/// # 돌려줌
	/// 사는 흔들림이 하나라도 있으면 true.
	///
	/// # 예제
	/// 늦게 시작할 흔들림도 남은 것으로 센다
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut camera_shake = CameraShake::new(1);
	/// assert!(!camera_shake.is_shaking());
	/// camera_shake.add(CameraShakeOptions { age: -0.5, ..Default::default() });
	/// assert!(camera_shake.is_shaking());
	/// ```
	//==============================================================================
	pub fn is_shaking(&self) -> bool {
		!self.shakes.is_empty()
	}

	//==============================================================================
	/// 모든 흔들림을 곧바로 멈추기.
	///
	/// # 예제
	/// 장면을 바꿀 때 흔들림 지우기
	/// ```rust
	/// use vanilla::{CameraShake, CameraShakeOptions};
	///
	/// let mut camera_shake = CameraShake::new(1);
	/// camera_shake.add(CameraShakeOptions::default());
	/// camera_shake.clear();
	/// assert!(!camera_shake.is_shaking());
	/// ```
	//==============================================================================
	pub fn clear(&mut self) {
		self.shakes.clear();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::graphics3d::camera3d::Camera3D;
	use serde_json::Value;

	/// 시험의 프레임 길이. (원본 시험과 같은 1/240 초)
	const FRAME_SECONDS: f64 = 1.0 / 240.0;

	//==============================================================================
	/// 덧붙임의 여섯 값. ([자리 x, y, z, 돌림 x, y, z])
	//==============================================================================
	fn read_values(offset: CameraShakeOffset) -> [f64; 6] {
		[offset.position.x, offset.position.y, offset.position.z, offset.rotation.x, offset.rotation.y, offset.rotation.z]
	}

	//==============================================================================
	/// 길이 동안 프레임마다 읽은 값들. (원본 시험의 recordShake, 시각을 프레임 길이씩 더한 차례도 같게)
	//==============================================================================
	fn record_shake(camera_shake: &mut CameraShake, seconds: f64) -> Vec<[f64; 6]> {
		let mut frames = Vec::new();
		let mut time = 0.0;
		while time < seconds {
			frames.push(read_values(camera_shake.read()));
			camera_shake.update(FRAME_SECONDS);
			time += FRAME_SECONDS;
		}
		frames
	}

	//==============================================================================
	/// 시간에 따라 잦아든다. (사분의 일마다 가장 큰 값이 줄고, 길이가 지나면 0 이고 흔들림이 끝난다)
	//==============================================================================
	#[test]
	fn shake_decays_over_time() {
		let mut camera_shake = CameraShake::new(3);
		camera_shake.add(CameraShakeOptions { amplitude: 1.0, rotation_amplitude: 0.1, duration: 1.0, frequency: 12.0, ..Default::default() });
		let frames = record_shake(&mut camera_shake, 1.0);
		let quarter_peaks: Vec<f64> = (0..4).map(|quarter| {
			let start = (frames.len() * quarter) / 4;
			let end = (frames.len() * (quarter + 1)) / 4;
			frames[start..end].iter().map(|values| trigonometry::hypot_list(&values[0..3])).fold(f64::NEG_INFINITY, f64::max)
		}).collect();
		assert!(quarter_peaks[0] > quarter_peaks[1] && quarter_peaks[1] > quarter_peaks[2] && quarter_peaks[2] > quarter_peaks[3], "{:?}", quarter_peaks);
		assert!(quarter_peaks[0] <= 3.0f64.sqrt() + 1e-9);
		let rotation_peak = frames.iter().map(|values| values[3].abs().max(values[4].abs()).max(values[5].abs())).fold(f64::NEG_INFINITY, f64::max);
		assert!(rotation_peak > 0.02 && rotation_peak <= 0.1 + 1e-12, "{}", rotation_peak);
		assert!(!camera_shake.is_shaking());
		assert!(read_values(camera_shake.read()).iter().all(|value| *value == 0.0));
	}

	//==============================================================================
	/// 같은 씨앗이면 같은 값, 다른 씨앗이면 다른 값.
	//==============================================================================
	#[test]
	fn same_seed_gives_same_values() {
		let mut frame_lists = Vec::new();
		for seed in [42, 42, 43] {
			let mut camera_shake = CameraShake::new(seed);
			camera_shake.add(CameraShakeOptions { amplitude: 0.2, rotation_amplitude: 0.05, duration: 0.5, ..Default::default() });
			camera_shake.add(CameraShakeOptions { amplitude: 0.1, duration: 0.3, age: 0.05, ..Default::default() });
			frame_lists.push(record_shake(&mut camera_shake, 0.5));
		}
		assert_eq!(frame_lists[0], frame_lists[1]);
		assert_ne!(frame_lists[0], frame_lists[2]);
	}

	//==============================================================================
	/// 겹침은 더한다. (둘을 같이 넣은 값은 따로 넣은 값의 합, 축 몫 0 이면 그 축은 0)
	//==============================================================================
	#[test]
	fn overlapping_shakes_add_up() {
		let shake_a = CameraShakeOptions { amplitude: 0.3, rotation_amplitude: 0.02, duration: 0.4, seed: Some(11), ..Default::default() };
		let shake_b = CameraShakeOptions { amplitude: 0.1, rotation_amplitude: 0.05, duration: 0.8, frequency: 4.0, position_axes: [1.0, 0.5, 0.0], seed: Some(22), ..Default::default() };
		let mut both = CameraShake::default();
		let mut only_a = CameraShake::default();
		let mut only_b = CameraShake::default();
		both.add(shake_a);
		both.add(shake_b);
		only_a.add(shake_a);
		only_b.add(shake_b);
		let both_frames = record_shake(&mut both, 0.8);
		let a_frames = record_shake(&mut only_a, 0.8);
		let b_frames = record_shake(&mut only_b, 0.8);
		let mut maximum_error = 0.0f64;
		for (frame_index, values) in both_frames.iter().enumerate() {
			for (channel_index, value) in values.iter().enumerate() {
				maximum_error = maximum_error.max((value - a_frames[frame_index][channel_index] - b_frames[frame_index][channel_index]).abs());
			}
		}
		assert!(maximum_error <= 1e-12, "{}", maximum_error);
		assert!(b_frames.iter().all(|values| values[2] == 0.0));
	}

	//==============================================================================
	/// 나이로 이어 붙이기. (0.3 초 늦게 나이 0.3 으로 넣은 흔들림은 처음부터 넣은 흔들림과 같은 값이 이어진다)
	//==============================================================================
	#[test]
	fn age_continues_late_shake() {
		let options = CameraShakeOptions { amplitude: 0.5, rotation_amplitude: 0.03, duration: 1.0, seed: Some(5), ..Default::default() };
		let mut from_start = CameraShake::default();
		from_start.add(options);
		from_start.update(0.3);
		let mut late = CameraShake::default();
		late.add(CameraShakeOptions { age: 0.3, ..options });
		assert_eq!(record_shake(&mut from_start, 0.5), record_shake(&mut late, 0.5));
	}

	//==============================================================================
	/// 늦은 시작과 오름. (나이가 음수면 0, attack 동안 0 에서 오른다, clear 뒤 끝남)
	//==============================================================================
	#[test]
	fn delayed_start_and_attack() {
		let mut camera_shake = CameraShake::default();
		camera_shake.add(CameraShakeOptions { amplitude: 1.0, duration: 1.0, age: -0.2, seed: Some(9), ..Default::default() });
		assert!(camera_shake.is_shaking());
		camera_shake.update(0.1);
		assert!(read_values(camera_shake.read()).iter().all(|value| *value == 0.0));
		camera_shake.update(0.15);
		assert!(read_values(camera_shake.read()).iter().any(|value| *value != 0.0));
		let mut attack_shake = CameraShake::default();
		attack_shake.add(CameraShakeOptions { amplitude: 1.0, duration: 1.0, attack: 0.2, seed: Some(9), ..Default::default() });
		assert!(read_values(attack_shake.read()).iter().all(|value| *value == 0.0));
		attack_shake.clear();
		assert!(!attack_shake.is_shaking());
	}

	//==============================================================================
	/// 보기 덧붙임의 축. (덧붙인 카메라의 자리는 카메라 지역 점, 고개를 든 pitch 는 앞이 위로, roll 은 앞이 그대로, yaw 는 위가 그대로)
	//==============================================================================
	#[test]
	fn offset_view_matrix_axes() {
		let view_matrix = Matrix4::create_look_at(Vector3::create(3.0, 2.0, 6.0), Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		let camera_matrix = Matrix4::create_inverse(&view_matrix);
		let position = Vector3::create(0.2, -0.1, 0.3);
		let pitch: f64 = 0.3;
		let shaken_camera_matrix = Matrix4::create_inverse(&create_offset_view_matrix(&view_matrix, position, Vector3::create(pitch, 0.0, 0.0)));
		let shaken_eye = shaken_camera_matrix.transform_point(0.0, 0.0, 0.0);
		let expected_eye = camera_matrix.transform_point(position.x, position.y, position.z);
		assert!((shaken_eye.x - expected_eye.x).abs() < 1e-5 && (shaken_eye.y - expected_eye.y).abs() < 1e-5 && (shaken_eye.z - expected_eye.z).abs() < 1e-5);
		let shaken_forward = shaken_camera_matrix.transform_direction(0.0, 0.0, -1.0);
		let expected_forward = camera_matrix.transform_direction(0.0, pitch.sin(), -pitch.cos());
		assert!((shaken_forward.x - expected_forward.x).abs() < 1e-5 && (shaken_forward.y - expected_forward.y).abs() < 1e-5 && (shaken_forward.z - expected_forward.z).abs() < 1e-5);
		let rolled = Matrix4::create_inverse(&create_offset_view_matrix(&view_matrix, Vector3::zero(), Vector3::create(0.0, 0.0, 0.4)));
		assert!((rolled.transform_direction(0.0, 0.0, -1.0).x - camera_matrix.transform_direction(0.0, 0.0, -1.0).x).abs() < 1e-5);
		let yawed = Matrix4::create_inverse(&create_offset_view_matrix(&view_matrix, Vector3::zero(), Vector3::create(0.0, 0.4, 0.0)));
		assert!((yawed.transform_direction(0.0, 1.0, 0.0).y - camera_matrix.transform_direction(0.0, 1.0, 0.0).y).abs() < 1e-5);
	}

	//==============================================================================
	/// Camera3D 덧붙임. (덧붙임 없음은 지금과 같고, 덧붙이면 create_offset_view_matrix 와 같고, 영으로 풀면 돌아온다, 원 행렬에도 건다)
	//==============================================================================
	#[test]
	fn camera_view_offset() {
		let mut camera = Camera3D::new();
		camera.look_at(Vector3::create(0.0, 4.0, 10.0), Vector3::zero(), Vector3::up());
		let plain_view = camera.get_view_matrix();
		assert_eq!(plain_view.get_elements(), Matrix4::create_look_at(Vector3::create(0.0, 4.0, 10.0), Vector3::zero(), Vector3::up()).get_elements());
		let position = Vector3::create(0.1, 0.05, 0.0);
		let rotation = Vector3::create(0.01, -0.02, 0.03);
		camera.set_view_offset(position, rotation);
		assert_eq!(camera.get_view_matrix().get_elements(), create_offset_view_matrix(&plain_view, position, rotation).get_elements());
		assert_eq!(camera.get_eye_position().x, 0.0);
		camera.set_view_offset(Vector3::zero(), Vector3::zero());
		assert_eq!(camera.get_view_matrix().get_elements(), plain_view.get_elements());
		camera.set_matrices(&plain_view, &Matrix4::create_perspective(0.8, 1.0, 0.1, 100.0));
		camera.set_view_offset(position, Vector3::zero());
		assert_eq!(camera.get_view_matrix().get_elements(), create_offset_view_matrix(&plain_view, position, Vector3::zero()).get_elements());
	}

	//==============================================================================
	/// JSON 숫자 배열을 f64 로. (원본이 찍은 기대값)
	//==============================================================================
	fn read_numbers(value: &Value) -> Vec<f64> {
		value.as_array().unwrap().iter().map(|number| number.as_f64().unwrap()).collect()
	}

	//==============================================================================
	/// 행렬 칸이 원본이 찍은 값과 1e-9 안인지.
	//==============================================================================
	fn assert_elements_near(matrix: &Matrix4, expected: &Value, label: &str) {
		let expected_numbers = read_numbers(expected);
		for (index, element) in matrix.get_elements().iter().enumerate() {
			assert!((*element as f64 - expected_numbers[index]).abs() <= 1e-9, "{} {} {} {}", label, index, element, expected_numbers[index]);
		}
	}

	//==============================================================================
	/// 두 엔진 견주기. (원본 cb59ad0 을 크롬에서 돌려 찍은 tests/assets/camerashake/chrome_camerashake.json 과 프레임마다 1e-9 안에서 같은지)
	//==============================================================================
	#[test]
	fn matches_vanilla_js_values() {
		let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/assets/camerashake/chrome_camerashake.json")).unwrap();
		let document: Value = serde_json::from_str(&text).unwrap();
		let read_axes = |value: &Value, fallback: [f64; 3]| -> [f64; 3] {
			match value.as_array() {
				Some(_) => {
					let numbers = read_numbers(value);
					[numbers[0], numbers[1], numbers[2]]
				}
				None => {
					fallback
				}
			}
		};
		let mut maximum_error = 0.0f64;
		let mut compared_count = 0;
		for shake_case in document["cases"].as_array().unwrap() {
			let mut camera_shake = match shake_case["baseSeed"].as_u64() {
				Some(seed) => {
					CameraShake::new(seed as u32)
				}
				None => {
					CameraShake::default()
				}
			};
			for shake in shake_case["shakes"].as_array().unwrap() {
				let defaults = CameraShakeOptions::default();
				camera_shake.add(CameraShakeOptions {
					amplitude: shake["amplitude"].as_f64().unwrap_or(defaults.amplitude),
					rotation_amplitude: shake["rotationAmplitude"].as_f64().unwrap_or(defaults.rotation_amplitude),
					position_axes: read_axes(&shake["positionAxes"], defaults.position_axes),
					rotation_axes: read_axes(&shake["rotationAxes"], defaults.rotation_axes),
					frequency: shake["frequency"].as_f64().unwrap_or(defaults.frequency),
					duration: shake["duration"].as_f64().unwrap_or(defaults.duration),
					attack: shake["attack"].as_f64().unwrap_or(defaults.attack),
					decay_exponent: shake["decayExponent"].as_f64().unwrap_or(defaults.decay_exponent),
					age: shake["age"].as_f64().unwrap_or(defaults.age),
					seed: shake["seed"].as_u64().map(|seed| seed as u32),
				});
			}
			let frame_seconds = shake_case["frameSeconds"].as_f64().unwrap();
			for frame in shake_case["frames"].as_array().unwrap() {
				let values = read_values(camera_shake.read());
				let expected = read_numbers(&frame["values"]);
				for channel_index in 0..6 {
					maximum_error = maximum_error.max((values[channel_index] - expected[channel_index]).abs());
					compared_count += 1;
				}
				assert_eq!(camera_shake.is_shaking(), frame["isShaking"].as_bool().unwrap(), "{}", shake_case["name"]);
				camera_shake.update(frame_seconds);
			}
		}
		assert!(compared_count > 600);
		assert!(maximum_error <= 1e-9, "{}", maximum_error);
		let offset_view = &document["offsetView"];
		let view_matrix = Matrix4::create_look_at(Vector3::create(3.0, 2.0, 6.0), Vector3::create(0.0, 1.0, 0.0), Vector3::create(0.0, 1.0, 0.0));
		assert_elements_near(&view_matrix, &offset_view["view"], "view");
		let offset_position = Vector3::create(0.2, -0.1, 0.3);
		let offset_rotation = Vector3::create(0.3, -0.2, 0.4);
		assert_elements_near(&create_offset_view_matrix(&view_matrix, offset_position, offset_rotation), &offset_view["result"], "offset view");
		let camera_values = &document["camera"];
		let mut camera = Camera3D::new();
		camera.look_at(Vector3::create(0.0, 4.0, 10.0), Vector3::zero(), Vector3::up());
		camera.set_view_offset(Vector3::create(0.1, 0.05, 0.0), Vector3::create(0.01, -0.02, 0.03));
		assert_elements_near(&camera.get_view_matrix(), &camera_values["view"], "camera view");
		assert_elements_near(&camera.get_view_projection_matrix(), &camera_values["viewProjection"], "camera view projection");
		camera.set_matrices(&view_matrix, &Matrix4::create_perspective(0.8, 1.0, 0.1, 100.0));
		camera.set_view_offset(offset_position, Vector3::zero());
		assert_elements_near(&camera.get_view_matrix(), &camera_values["rawView"], "raw camera view");
		eprintln!("camerashake 두 엔진 견주기: 값 {} 개, 가장 큰 차 {:e}", compared_count, maximum_error);
	}
}
