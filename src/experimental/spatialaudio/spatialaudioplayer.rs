//==============================================================================
//! 공간 소리 플레이어. (원본 src/experimental/spatialaudio/spatialaudioplayer.js)
//! - 초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다.
//! - AudioPlayer 의 GainNode 뒤에 PannerNode 를 끼운다. (소스 → GainNode → PannerNode → 버스나 출력) 음량, 음소거, 루프, 재생 위치, 빠르기는 AudioPlayer 그대로다.
//! - 버스를 주면 그 버스로 낸다. SoundSystem 의 효과음 버스(get_effect_bus)로 내면 설정의 효과음 크기와 음소거를 따른다.
//! - 자리와 방향은 PannerNode 의 positionX ~ orientationZ 에 값으로 대입한다. (바뀐 칸만, 다음 렌더 퀀텀부터)
//! - 설정은 WebAudio 가 받는 값만 받는다. (기준 거리 0 이상, 최대 거리 0 보다 큼, rolloff 0 이상, 원뿔 바깥 이득 0 ~ 1, 모두 유한) 아니면 거짓을 돌려주고 앞 값을 둔다.
//==============================================================================
use super::spatialaudiolistener::SpatialAudioListener;
use crate::base::vector3::Vector3;
use crate::core::audiobus::AudioBus;
use crate::core::audiomanager::AudioManager;
use crate::core::audioplayer::AudioPlayer;
use crate::platform::audio::pannermath::{calculate_distance_cone_gain, DistanceModel, PannerSettings, PanningModel};
use crate::platform::audio::PannerParam;
use crate::resource::audioasset::AudioAsset;
use std::rc::Rc;

//==============================================================================
/// 공간 소리 플레이어.
/// # 예제
/// ```
/// use vanilla::experimental::spatialaudio::{PanningModel, SpatialAudioPlayer};
/// use vanilla::{AudioAsset, AudioManager, Vector3};
/// use std::rc::Rc;
///
/// let audio_manager = AudioManager::new();
/// let mut player = SpatialAudioPlayer::new(&audio_manager, None);
/// player.set_audio_asset(Rc::new(AudioAsset::new()));
/// player.set_panning_model(PanningModel::Hrtf);
/// assert!(player.set_cone(90.0, 240.0, 0.3));
/// assert!(!player.set_reference_distance(-1.0));
/// player.set_position(Vector3::create(2.0, 0.0, -1.0));
/// player.set_orientation(Vector3::create(0.0, 0.0, 1.0));
/// player.set_volume(0.8);
/// player.play(true);
/// assert_eq!(player.get_position(), Vector3::create(2.0, 0.0, -1.0));
/// assert_eq!(player.get_settings().cone_outer_gain, 0.3);
/// ```
//==============================================================================
pub struct SpatialAudioPlayer {
	audio_player: AudioPlayer,
	settings: PannerSettings,
	position: Vector3,
	orientation: Vector3,
}

impl SpatialAudioPlayer {
	//==============================================================================
	/// 생성. (버스를 주면 그 버스로, 없으면 출력으로 낸다. AudioContext 가 없으면 소리 없는 플레이어)
	/// - PannerNode 는 WebAudio 기본값이다. (등력, 역수, 기준 1, 최대 10000, rolloff 1, 원뿔 360 / 360 / 0, 자리 (0, 0, 0), 방향 (1, 0, 0))
	///
	/// # 인자
	/// - `audio_manager`: 엔진의 AudioManager. (AudioContext 가 없으면 만든다)
	/// - `audio_bus`: 소리를 낼 버스. (None 이면 출력)
	///
	/// # 돌려줌
	/// 새 플레이어.
	//==============================================================================
	pub fn new(audio_manager: &AudioManager, audio_bus: Option<&AudioBus>) -> SpatialAudioPlayer {
		let mut audio_player = match audio_bus {
			Some(audio_bus) => {
				audio_bus.create_audio_player()
			}
			None => {
				audio_manager.create_audio_player()
			}
		};
		let settings = PannerSettings::default();
		audio_player.set_panner_settings(settings);
		SpatialAudioPlayer {
			audio_player,
			settings,
			position: Vector3::create(0.0, 0.0, 0.0),
			orientation: Vector3::create(1.0, 0.0, 0.0),
		}
	}

	//==============================================================================
	/// 안의 AudioPlayer. (음소거, 재생 위치, 빠르기 같은 나머지 기능)
	///
	/// # 돌려줌
	/// 안의 플레이어.
	//==============================================================================
	pub fn get_audio_player(&self) -> &AudioPlayer {
		&self.audio_player
	}

	//==============================================================================
	/// 안의 AudioPlayer. (변경용)
	///
	/// # 돌려줌
	/// 안의 플레이어. (변경용)
	//==============================================================================
	pub fn get_audio_player_mut(&mut self) -> &mut AudioPlayer {
		&mut self.audio_player
	}

	//==============================================================================
	/// 틀 소리 애셋 설정. (다음 play 부터)
	///
	/// # 인자
	/// - `audio_asset`: 틀 소리 애셋.
	//==============================================================================
	pub fn set_audio_asset(&mut self, audio_asset: Rc<AudioAsset>) {
		self.audio_player.set_audio_asset(audio_asset);
	}

	//==============================================================================
	/// 재생. (is_loop 가 참이면 되풀이, AudioPlayer::play)
	///
	/// # 인자
	/// - `is_loop`: 되풀이하면 true.
	//==============================================================================
	pub fn play(&mut self, is_loop: bool) {
		self.audio_player.play(is_loop);
	}

	//==============================================================================
	/// 정지.
	//==============================================================================
	pub fn stop(&mut self) {
		self.audio_player.stop();
	}

	//==============================================================================
	/// 재생 중인지. (끝까지 틀면 거짓)
	///
	/// # 돌려줌
	/// 재생 중이면 true.
	//==============================================================================
	pub fn is_playing(&self) -> bool {
		self.audio_player.is_playing()
	}

	//==============================================================================
	/// 음량 설정. (0 ~ 1 로 자르고, 유한하지 않으면 1. 거리와 원뿔 이득 앞의 GainNode)
	///
	/// # 인자
	/// - `volume`: 음량.
	//==============================================================================
	pub fn set_volume(&mut self, volume: f32) {
		self.audio_player.set_volume(volume);
	}

	//==============================================================================
	/// 음량. (0 ~ 1, 기본 1)
	///
	/// # 돌려줌
	/// 음량.
	//==============================================================================
	pub fn get_volume(&self) -> f32 {
		self.audio_player.get_volume()
	}

	//==============================================================================
	/// 자리 설정. (WebAudio 좌표, 유한하지 않은 칸은 받지 않는다)
	///
	/// # 인자
	/// - `position`: 자리.
	//==============================================================================
	pub fn set_position(&mut self, position: Vector3) {
		let previous = self.position;
		self.position = self.write_vector(previous, position, [PannerParam::PositionX, PannerParam::PositionY, PannerParam::PositionZ]);
	}

	//==============================================================================
	/// 자리. (기본 (0, 0, 0))
	///
	/// # 돌려줌
	/// 자리.
	//==============================================================================
	pub fn get_position(&self) -> Vector3 {
		self.position
	}

	//==============================================================================
	/// 소리가 보는 방향 설정. (원뿔의 축, 길이는 상관없고 (0, 0, 0)이면 원뿔을 쓰지 않는다, 유한하지 않은 칸은 받지 않는다)
	///
	/// # 인자
	/// - `orientation`: 방향.
	//==============================================================================
	pub fn set_orientation(&mut self, orientation: Vector3) {
		let previous = self.orientation;
		self.orientation = self.write_vector(previous, orientation, [PannerParam::OrientationX, PannerParam::OrientationY, PannerParam::OrientationZ]);
	}

	//==============================================================================
	/// 소리가 보는 방향. (기본 (1, 0, 0))
	///
	/// # 돌려줌
	/// 방향.
	//==============================================================================
	pub fn get_orientation(&self) -> Vector3 {
		self.orientation
	}

	//==============================================================================
	/// 바뀐 칸만 AudioParam 에 대입하고 새 값을 돌려준다.
	///
	/// # 인자
	/// - `previous`: 앞 값.
	/// - `next`: 새 값.
	/// - `param_list`: AudioParam 이름 셋.
	///
	/// # 돌려줌
	/// 받아들인 값.
	//==============================================================================
	fn write_vector(&mut self, previous: Vector3, next: Vector3, param_list: [PannerParam; 3]) -> Vector3 {
		let previous_list = [previous.x, previous.y, previous.z];
		let mut next_list = [next.x, next.y, next.z];
		for index in 0..3 {
			if !next_list[index].is_finite() {
				next_list[index] = previous_list[index];
				continue;
			}
			if next_list[index] != previous_list[index] {
				self.audio_player.set_panner_param_value(param_list[index], next_list[index]);
			}
		}
		Vector3::create(next_list[0], next_list[1], next_list[2])
	}

	//==============================================================================
	/// 설정 한꺼번에. (WebAudio 가 받지 않는 값이 하나라도 있으면 거짓을 돌려주고 아무것도 바꾸지 않는다)
	///
	/// # 인자
	/// - `settings`: 설정.
	///
	/// # 돌려줌
	/// 받았으면 true.
	//==============================================================================
	pub fn set_settings(&mut self, settings: PannerSettings) -> bool {
		if !settings.is_valid() {
			return false;
		}
		self.settings = settings;
		self.audio_player.set_panner_settings(settings);
		true
	}

	//==============================================================================
	/// 설정.
	///
	/// # 돌려줌
	/// 설정.
	//==============================================================================
	pub fn get_settings(&self) -> PannerSettings {
		self.settings
	}

	//==============================================================================
	/// 좌우 방식 설정. (데스크탑의 HRTF 는 아직 등력으로 셈한다)
	///
	/// # 인자
	/// - `panning_model`: 좌우 방식.
	//==============================================================================
	pub fn set_panning_model(&mut self, panning_model: PanningModel) {
		self.set_settings(PannerSettings { panning_model, ..self.settings });
	}

	//==============================================================================
	/// 거리 모델 설정. (크롬처럼 이것만 바꾸면 자리나 다른 거리 값이 바뀔 때까지 앞 이득을 쓴다)
	///
	/// # 인자
	/// - `distance_model`: 거리 모델.
	//==============================================================================
	pub fn set_distance_model(&mut self, distance_model: DistanceModel) {
		self.set_settings(PannerSettings { distance_model, ..self.settings });
	}

	//==============================================================================
	/// 기준 거리 설정. (0 이상, 이 거리 안에서는 줄지 않는다, 받지 않으면 거짓)
	///
	/// # 인자
	/// - `reference_distance`: 기준 거리.
	///
	/// # 돌려줌
	/// 받았으면 true.
	//==============================================================================
	pub fn set_reference_distance(&mut self, reference_distance: f64) -> bool {
		self.set_settings(PannerSettings { reference_distance, ..self.settings })
	}

	//==============================================================================
	/// 최대 거리 설정. (0 보다 큼, linear 는 여기서 가장 작아지고 나머지는 여기서 더 줄지 않는다, 받지 않으면 거짓)
	///
	/// # 인자
	/// - `maximum_distance`: 최대 거리.
	///
	/// # 돌려줌
	/// 받았으면 true.
	//==============================================================================
	pub fn set_maximum_distance(&mut self, maximum_distance: f64) -> bool {
		self.set_settings(PannerSettings { maximum_distance, ..self.settings })
	}

	//==============================================================================
	/// 줄어드는 빠르기 설정. (0 이상, 받지 않으면 거짓)
	///
	/// # 인자
	/// - `rolloff_factor`: 줄어드는 빠르기.
	///
	/// # 돌려줌
	/// 받았으면 true.
	//==============================================================================
	pub fn set_rolloff_factor(&mut self, rolloff_factor: f64) -> bool {
		self.set_settings(PannerSettings { rolloff_factor, ..self.settings })
	}

	//==============================================================================
	/// 원뿔 설정. (안쪽 각과 바깥쪽 각은 도로 잰 각 전체, 바깥 이득은 0 ~ 1, 받지 않으면 거짓)
	///
	/// # 인자
	/// - `inner_angle`: 안쪽 각.
	/// - `outer_angle`: 바깥쪽 각.
	/// - `outer_gain`: 바깥 이득.
	///
	/// # 돌려줌
	/// 받았으면 true.
	//==============================================================================
	pub fn set_cone(&mut self, inner_angle: f64, outer_angle: f64, outer_gain: f64) -> bool {
		self.set_settings(PannerSettings { cone_inner_angle: inner_angle, cone_outer_angle: outer_angle, cone_outer_gain: outer_gain, ..self.settings })
	}

	//==============================================================================
	/// 지금 자리와 설정으로 본 거리 x 원뿔 이득. (크롬과 같은 셈, 음량과 버스는 넣지 않는다. 멀어 들리지 않는 소리를 거를 때)
	///
	/// # 인자
	/// - `listener`: 리스너.
	///
	/// # 돌려줌
	/// 거리 x 원뿔 이득.
	//==============================================================================
	pub fn calculate_gain(&self, listener: &SpatialAudioListener) -> f32 {
		calculate_distance_cone_gain(&self.settings, self.position, self.orientation, listener.get_position())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 받지 않는 설정은 앞 값을 둔다. 유한하지 않은 자리 칸도 그렇다.
	//==============================================================================
	#[test]
	fn invalid_values_keep_previous_settings() {
		let audio_manager = AudioManager::new();
		let mut player = SpatialAudioPlayer::new(&audio_manager, None);
		assert!(player.set_maximum_distance(50.0));
		assert!(!player.set_maximum_distance(0.0));
		assert!(!player.set_rolloff_factor(-0.5));
		assert!(!player.set_cone(30.0, 60.0, 1.5));
		assert!(!player.set_reference_distance(f64::NAN));
		let settings = player.get_settings();
		assert_eq!(settings.maximum_distance, 50.0);
		assert_eq!(settings.rolloff_factor, 1.0);
		assert_eq!(settings.cone_outer_gain, 0.0);
		player.set_position(Vector3::create(1.0, f32::NAN, 3.0));
		assert_eq!(player.get_position(), Vector3::create(1.0, 0.0, 3.0));
	}
}
