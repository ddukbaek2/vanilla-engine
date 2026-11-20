//==============================================================================
// 소리 표로 다루는 배경음악과 효과음. (vanilla.js core/soundsystem.js 의 SoundSystem)
// - AudioManager 의 플레이어와 AudioAsset 을 쓴다. 소리를 합성하지 않고 파일만 튼다.
// - 소리 표의 줄마다 SoundRow { id, paths, volume, minimum_interval, is_music, maximum_voices, pitch_variation, play_seconds, fade_out_seconds }.
//   paths: 같은 소리를 여러 번 녹음한 무리. 낼 때마다 하나를 고르고 바로 앞과 같은 것은 고르지 않는다.
//   minimum_interval: 같은 소리를 다시 낼 수 있는 간격. (초, 한꺼번에 몰려 시끄럽지 않게)
//   maximum_voices: 같은 소리가 한꺼번에 날 수 있는 수. (넘으면 가장 오래된 것을 끊는다, 0 이면 효과음 전체의 수까지)
//   pitch_variation: 낼 때마다 음 높이를 흔드는 비. (0.05 면 ±5%)
//   play_seconds, fade_out_seconds: 효과음을 앞의 play_seconds 초만 틀고 fade_out_seconds 초 동안 줄여 끈다. (0 이면 끝까지)
// - 크기: 배경음악 = 음악 크기 × 음악의 비 × 소리의 크기, 효과음 = 효과음 크기 × 소리의 크기 × 부를 때의 크기.
// - 배경음악은 겹쳐 바꿀 수 있고(fade_seconds), 멈췄던 자리를 소리마다 기억해 이어 틀 수 있다. (is_resumed)
// - 음소거는 설정한 크기보다 먼저다. 사용자의 음소거(set_user_muted), 페이지 가려짐(set_page_hidden),
//   바깥의 음소거(set_external_mute_provider, 플랫폼 SDK 나 광고) 가운데 하나라도 맞으면 소리가 나지 않는다.
// - 소리 파일은 놀이를 막지 않게 뒤에서 받는다. (웹은 디코딩이 끝나면 난다) 못 받은 소리는 나지 않을 뿐이다.
// - 원본과 다른 것: 음소거 동안 AudioContext 를 멈추지 않고 플레이어 크기를 0 으로 둔다. (러스트 오디오 백엔드에 컨텍스트 멈춤이 없다)
//   음악과 효과음 버스(GainNode) 대신 플레이어마다 버스 크기를 곱해 건다. 페이지 가려짐은 부르는 쪽이 알린다.
//==============================================================================
use std::collections::HashMap;
use std::rc::Rc;

use crate::base::seededrandom::SeededRandom;
use crate::core::audioplayer::AudioPlayer;
use crate::core::engine::Engine;
use crate::resource::audioasset::AudioAsset;

// 효과음을 동시에 낼 수 있는 수. (넘으면 가장 오래된 것을 끊는다)
const EFFECT_VOICE_COUNT: usize = 12;

//==============================================================================
// 소리 표의 한 줄.
//==============================================================================
#[derive(Clone, Debug)]
pub struct SoundRow {
	pub id: String,
	pub paths: Vec<String>,
	pub volume: f32,
	pub minimum_interval: f32,
	pub is_music: bool,
	pub maximum_voices: usize,
	pub pitch_variation: f32,
	pub play_seconds: f32,
	pub fade_out_seconds: f32,
}

impl SoundRow {
	//==============================================================================
	// 효과음 한 줄. (나머지 칸은 기본)
	//==============================================================================
	pub fn create_effect(id: &str, paths: &[&str], volume: f32) -> SoundRow {
		SoundRow {
			id: String::from(id),
			paths: paths.iter().map(|path| String::from(*path)).collect(),
			volume,
			minimum_interval: 0.0,
			is_music: false,
			maximum_voices: 0,
			pitch_variation: 0.0,
			play_seconds: 0.0,
			fade_out_seconds: 0.0,
		}
	}

	//==============================================================================
	// 배경음악 한 줄.
	//==============================================================================
	pub fn create_music(id: &str, path: &str, volume: f32) -> SoundRow {
		let mut sound_row = SoundRow::create_effect(id, &[path], volume);
		sound_row.is_music = true;
		sound_row
	}
}

//==============================================================================
// 내고 있는 효과음 하나.
//==============================================================================
struct EffectVoice {
	audio_player: AudioPlayer,
	sound_id: String,
	volume: f32,
	elapsed_seconds: f32,
	play_seconds: f32,
	fade_out_seconds: f32,
	fade_start_seconds: f32,
}

//==============================================================================
// 내고 있는 배경음악 하나. (겹쳐 바꾸는 동안 둘)
//==============================================================================
struct MusicVoice {
	audio_player: AudioPlayer,
	sound_id: String,
	volume: f32,
	fade_ratio: f32,
	fade_seconds: f32,
	is_fading_in: bool,
}

pub struct SoundSystem {
	sound_by_id: HashMap<String, SoundRow>,
	asset_by_id: HashMap<String, Vec<Rc<AudioAsset>>>,
	last_variant_by_id: HashMap<String, usize>,
	last_play_time_by_id: HashMap<String, f32>,
	effect_voice_list: Vec<EffectVoice>,
	music_voice_list: Vec<MusicVoice>,
	music_resume_time_by_id: HashMap<String, f64>,
	music_volume: f32,
	music_scale: f32,
	sound_effect_volume: f32,
	is_user_muted: bool,
	is_page_hidden: bool,
	is_muted: bool,
	external_mute_provider: Option<Box<dyn Fn() -> bool>>,
	elapsed_seconds: f32,
	random: SeededRandom,
}

impl SoundSystem {
	//==============================================================================
	// 생성. (소리 표를 받는다, 받기는 load_sounds 가 한다)
	//==============================================================================
	pub fn new(sound_table: Vec<SoundRow>) -> SoundSystem {
		let mut sound_system = SoundSystem {
			sound_by_id: HashMap::new(),
			asset_by_id: HashMap::new(),
			last_variant_by_id: HashMap::new(),
			last_play_time_by_id: HashMap::new(),
			effect_voice_list: Vec::new(),
			music_voice_list: Vec::new(),
			music_resume_time_by_id: HashMap::new(),
			music_volume: 1.0,
			music_scale: 1.0,
			sound_effect_volume: 1.0,
			is_user_muted: false,
			is_page_hidden: false,
			is_muted: false,
			external_mute_provider: None,
			elapsed_seconds: 0.0,
			random: SeededRandom::new(0x50f7),
		};
		sound_system.add_sounds(sound_table);
		sound_system
	}

	//==============================================================================
	// 소리 표의 줄 더하기. (같은 id 는 바꾼다, 받기는 load_sounds 가 한다)
	//==============================================================================
	pub fn add_sounds(&mut self, sound_table: Vec<SoundRow>) {
		for sound_row in sound_table {
			self.sound_by_id.insert(sound_row.id.clone(), sound_row);
		}
	}

	//==============================================================================
	// 소리 파일 받기. (아직 받지 않은 줄만, 애셋 폴더 기준 경로, 못 받은 파일은 알리고 지나간다)
	//==============================================================================
	pub fn load_sounds(&mut self, engine: &Engine) {
		for (sound_id, sound_row) in self.sound_by_id.iter() {
			if self.asset_by_id.contains_key(sound_id) {
				continue;
			}
			let mut asset_list = Vec::new();
			for path in sound_row.paths.iter() {
				match engine.load_audio_asset(path) {
					Ok(audio_asset) => {
						asset_list.push(audio_asset);
					}
					Err(error) => {
						eprintln!("[sound] 소리를 받지 못했습니다. {} ({})", path, error);
					}
				}
			}
			self.asset_by_id.insert(sound_id.clone(), asset_list);
		}
	}

	//==============================================================================
	// 소리 애셋 직접 더하기. (메모리에서 지은 소리처럼 파일로 받지 않는 것, 같은 id 의 녹음 무리에 덧붙인다)
	//==============================================================================
	pub fn add_sound_asset(&mut self, sound_id: &str, audio_asset: Rc<AudioAsset>) {
		self.asset_by_id.entry(String::from(sound_id)).or_default().push(audio_asset);
	}

	//==============================================================================
	// 바깥의 음소거를 읽는 함수 설정. (플랫폼 SDK 의 음소거, 보상형 광고를 보이는 동안 등, 매 프레임 읽는다)
	//==============================================================================
	pub fn set_external_mute_provider(&mut self, external_mute_provider: Option<Box<dyn Fn() -> bool>>) {
		self.external_mute_provider = external_mute_provider;
	}

	//==============================================================================
	// 사용자의 음소거 설정.
	//==============================================================================
	pub fn set_user_muted(&mut self, is_user_muted: bool) {
		self.is_user_muted = is_user_muted;
	}

	pub fn is_user_muted(&self) -> bool {
		self.is_user_muted
	}

	//==============================================================================
	// 페이지가 가려졌는지 설정. (다른 탭, 앱 전환, 창이 가려짐)
	//==============================================================================
	pub fn set_page_hidden(&mut self, is_page_hidden: bool) {
		self.is_page_hidden = is_page_hidden;
	}

	//==============================================================================
	// 지금 음소거여야 하는지. (바깥의 음소거, 사용자의 음소거, 페이지 가려짐)
	//==============================================================================
	pub fn read_muted(&self) -> bool {
		let is_external_muted = self.external_mute_provider.as_ref().map(|provider| provider()).unwrap_or(false);
		is_external_muted || self.is_user_muted || self.is_page_hidden
	}

	//==============================================================================
	// 크기 설정. (음악 크기, 음악의 비(멈춤 창의 반 크기 등), 효과음 크기, 모두 0 ~ 1)
	//==============================================================================
	pub fn set_music_volume(&mut self, music_volume: f32) {
		self.music_volume = music_volume.clamp(0.0, 1.0);
	}

	pub fn get_music_volume(&self) -> f32 {
		self.music_volume
	}

	pub fn set_music_scale(&mut self, music_scale: f32) {
		self.music_scale = music_scale.clamp(0.0, 1.0);
	}

	pub fn set_sound_effect_volume(&mut self, sound_effect_volume: f32) {
		self.sound_effect_volume = sound_effect_volume.clamp(0.0, 1.0);
	}

	pub fn get_sound_effect_volume(&self) -> f32 {
		self.sound_effect_volume
	}

	//==============================================================================
	// 지금 배경음악의 id. (없으면 None)
	//==============================================================================
	pub fn get_music_id(&self) -> Option<&str> {
		self.music_voice_list.iter().rev().find(|music_voice| music_voice.is_fading_in).map(|music_voice| music_voice.sound_id.as_str())
	}

	//==============================================================================
	// 녹음 무리에서 하나 고르기. (바로 앞과 같은 것은 고르지 않는다)
	//==============================================================================
	fn pick_variant(&mut self, sound_id: &str, variant_count: usize) -> usize {
		if variant_count <= 1 {
			return 0;
		}
		let last_variant = self.last_variant_by_id.get(sound_id).copied();
		let mut variant_index = (self.random.next_value() * variant_count as f64) as usize % variant_count;
		if Some(variant_index) == last_variant {
			variant_index = (variant_index + 1) % variant_count;
		}
		self.last_variant_by_id.insert(String::from(sound_id), variant_index);
		variant_index
	}

	//==============================================================================
	// 효과음 내기. (부를 때의 크기 0 ~ 1)
	//==============================================================================
	pub fn play_effect(&mut self, engine: &Engine, sound_id: &str, volume: f32) {
		if self.is_muted {
			return;
		}
		let sound_row = match self.sound_by_id.get(sound_id) {
			Some(sound_row) => {
				sound_row.clone()
			}
			None => {
				return;
			}
		};
		let asset_list = match self.asset_by_id.get(sound_id) {
			Some(asset_list) if !asset_list.is_empty() => {
				asset_list.clone()
			}
			_ => {
				return;
			}
		};
		if let Some(last_play_time) = self.last_play_time_by_id.get(sound_id) {
			if self.elapsed_seconds - last_play_time < sound_row.minimum_interval {
				return;
			}
		}
		let variant_index = self.pick_variant(sound_id, asset_list.len());
		let audio_asset = asset_list[variant_index].clone();
		if !audio_asset.is_loaded() {
			return;
		}
		self.last_play_time_by_id.insert(String::from(sound_id), self.elapsed_seconds);

		// 같은 소리의 수와 전체 수를 넘으면 가장 오래된 것을 끊는다.
		let maximum_voices = if sound_row.maximum_voices > 0 { sound_row.maximum_voices } else { EFFECT_VOICE_COUNT };
		self.trim_voices(Some(sound_id), maximum_voices - 1);
		self.trim_voices(None, EFFECT_VOICE_COUNT - 1);
		let mut audio_player = engine.get_audio_manager().create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		if sound_row.pitch_variation > 0.0 {
			let pitch_ratio = 1.0 + (self.random.next_value() as f32 * 2.0 - 1.0) * sound_row.pitch_variation;
			audio_player.set_playback_rate(pitch_ratio as f64);
		}
		let voice_volume = sound_row.volume * volume.clamp(0.0, 1.0);
		audio_player.set_volume(voice_volume * self.sound_effect_volume);
		audio_player.play(false);
		self.effect_voice_list.push(EffectVoice {
			audio_player,
			sound_id: String::from(sound_id),
			volume: voice_volume,
			elapsed_seconds: 0.0,
			play_seconds: sound_row.play_seconds,
			fade_out_seconds: sound_row.fade_out_seconds,
			fade_start_seconds: -1.0,
		});
	}

	//==============================================================================
	// 내는 효과음을 keep_count 개만 남기고 오래된 것부터 끊기. (sound_id 가 없으면 모두에서)
	//==============================================================================
	fn trim_voices(&mut self, sound_id: Option<&str>, keep_count: usize) {
		let voice_count = self.effect_voice_list.iter().filter(|voice| sound_id.map(|id| voice.sound_id == id).unwrap_or(true)).count();
		let mut remove_count = voice_count.saturating_sub(keep_count);
		self.effect_voice_list.retain_mut(|voice| {
			if remove_count == 0 || !sound_id.map(|id| voice.sound_id == id).unwrap_or(true) {
				return true;
			}
			voice.audio_player.stop();
			remove_count -= 1;
			false
		});
	}

	//==============================================================================
	// 한 효과음을 줄여 끄기. (fade_seconds 초 동안)
	//==============================================================================
	pub fn fade_out_effects(&mut self, sound_id: &str, fade_seconds: f32) {
		for voice in self.effect_voice_list.iter_mut() {
			if voice.sound_id == sound_id && voice.fade_start_seconds < 0.0 {
				voice.fade_start_seconds = voice.elapsed_seconds;
				voice.fade_out_seconds = fade_seconds.max(0.001);
			}
		}
	}

	//==============================================================================
	// 효과음 모두 끄기.
	//==============================================================================
	pub fn stop_effects(&mut self) {
		for voice in self.effect_voice_list.iter_mut() {
			voice.audio_player.stop();
		}
		self.effect_voice_list.clear();
	}

	//==============================================================================
	// 배경음악 틀기. (fade_seconds 동안 앞 음악과 겹쳐 바꾼다, is_resumed 면 그 음악을 멈췄던 자리에서 잇는다)
	//==============================================================================
	pub fn play_music(&mut self, engine: &Engine, sound_id: &str, fade_seconds: f32, is_resumed: bool) {
		if self.get_music_id() == Some(sound_id) {
			return;
		}
		let sound_row = match self.sound_by_id.get(sound_id) {
			Some(sound_row) => {
				sound_row.clone()
			}
			None => {
				return;
			}
		};
		let audio_asset = match self.asset_by_id.get(sound_id).and_then(|asset_list| asset_list.first()) {
			Some(audio_asset) => {
				audio_asset.clone()
			}
			None => {
				return;
			}
		};
		self.fade_out_music(fade_seconds);
		let mut audio_player = engine.get_audio_manager().create_audio_player();
		audio_player.set_audio_asset(audio_asset);
		if is_resumed {
			let resume_time = self.music_resume_time_by_id.get(sound_id).copied().unwrap_or(0.0);
			audio_player.set_time(resume_time);
		}
		let start_ratio = if fade_seconds > 0.0 { 0.0 } else { 1.0 };
		audio_player.set_volume(0.0);
		audio_player.play(true);
		self.music_voice_list.push(MusicVoice {
			audio_player,
			sound_id: String::from(sound_id),
			volume: sound_row.volume,
			fade_ratio: start_ratio,
			fade_seconds: fade_seconds.max(0.0),
			is_fading_in: true,
		});
		self.apply_volumes();
	}

	//==============================================================================
	// 배경음악 줄여 끄기. (멈춘 자리를 기억한다)
	//==============================================================================
	pub fn fade_out_music(&mut self, fade_seconds: f32) {
		for music_voice in self.music_voice_list.iter_mut() {
			if !music_voice.is_fading_in {
				continue;
			}
			music_voice.is_fading_in = false;
			music_voice.fade_seconds = fade_seconds.max(0.0);
			let resume_time = music_voice.audio_player.get_time();
			self.music_resume_time_by_id.insert(music_voice.sound_id.clone(), resume_time);
		}
	}

	//==============================================================================
	// 매 프레임. (음소거 판정, 효과음의 앞 몇 초와 줄여 끄기, 음악 겹쳐 바꾸기, 끝난 소리 치우기)
	//==============================================================================
	pub fn update(&mut self, time_delta: f32) {
		self.elapsed_seconds += time_delta;
		self.is_muted = self.read_muted();
		for voice in self.effect_voice_list.iter_mut() {
			voice.elapsed_seconds += time_delta;
			if voice.play_seconds > 0.0 && voice.fade_start_seconds < 0.0 && voice.elapsed_seconds >= voice.play_seconds {
				voice.fade_start_seconds = voice.elapsed_seconds;
				voice.fade_out_seconds = voice.fade_out_seconds.max(0.001);
			}
		}
		for music_voice in self.music_voice_list.iter_mut() {
			let step = if music_voice.fade_seconds > 0.0 { time_delta / music_voice.fade_seconds } else { 1.0 };
			music_voice.fade_ratio = if music_voice.is_fading_in { (music_voice.fade_ratio + step).min(1.0) } else { (music_voice.fade_ratio - step).max(0.0) };
		}
		self.effect_voice_list.retain_mut(|voice| {
			let is_faded_out = voice.fade_start_seconds >= 0.0 && voice.elapsed_seconds - voice.fade_start_seconds >= voice.fade_out_seconds;
			if is_faded_out {
				voice.audio_player.stop();
				return false;
			}
			voice.audio_player.is_playing() || voice.elapsed_seconds < 0.5
		});
		self.music_voice_list.retain_mut(|music_voice| {
			if !music_voice.is_fading_in && music_voice.fade_ratio <= 0.0 {
				music_voice.audio_player.stop();
				return false;
			}
			true
		});
		self.apply_volumes();
	}

	//==============================================================================
	// 플레이어마다 크기 걸기. (음소거면 0)
	//==============================================================================
	fn apply_volumes(&mut self) {
		let mute_scale = if self.is_muted { 0.0 } else { 1.0 };
		let sound_effect_volume = self.sound_effect_volume * mute_scale;
		for voice in self.effect_voice_list.iter_mut() {
			let fade_scale = if voice.fade_start_seconds >= 0.0 { (1.0 - (voice.elapsed_seconds - voice.fade_start_seconds) / voice.fade_out_seconds).clamp(0.0, 1.0) } else { 1.0 };
			voice.audio_player.set_volume(voice.volume * sound_effect_volume * fade_scale);
		}
		let music_volume = self.music_volume * self.music_scale * mute_scale;
		for music_voice in self.music_voice_list.iter_mut() {
			music_voice.audio_player.set_volume(music_voice.volume * music_volume * music_voice.fade_ratio);
		}
	}

	//==============================================================================
	// 내고 있는 효과음 수. (시험과 기록용)
	//==============================================================================
	pub fn get_effect_voice_count(&self) -> usize {
		self.effect_voice_list.len()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::core::engine::EngineConfiguration;

	//==============================================================================
	// 짧은 무음 WAV. (16 비트 모노 8000 Hz, 0.2 초)
	//==============================================================================
	fn create_silent_wav() -> Vec<u8> {
		let sample_count: u32 = 1600;
		let data_size = sample_count * 2;
		let mut bytes = Vec::new();
		bytes.extend_from_slice(b"RIFF");
		bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
		bytes.extend_from_slice(b"WAVEfmt ");
		bytes.extend_from_slice(&16u32.to_le_bytes());
		bytes.extend_from_slice(&1u16.to_le_bytes());
		bytes.extend_from_slice(&1u16.to_le_bytes());
		bytes.extend_from_slice(&8000u32.to_le_bytes());
		bytes.extend_from_slice(&16000u32.to_le_bytes());
		bytes.extend_from_slice(&2u16.to_le_bytes());
		bytes.extend_from_slice(&16u16.to_le_bytes());
		bytes.extend_from_slice(b"data");
		bytes.extend_from_slice(&data_size.to_le_bytes());
		bytes.extend(std::iter::repeat_n(0u8, data_size as usize));
		bytes
	}

	#[test]
	fn interval_and_voice_limit() {
		let engine = Engine::new(EngineConfiguration::new());
		let mut hit_row = SoundRow::create_effect("hit", &[], 1.0);
		hit_row.minimum_interval = 0.1;
		hit_row.maximum_voices = 2;
		let mut sound_system = SoundSystem::new(vec![hit_row]);
		let audio_asset = Rc::new(AudioAsset::new());
		assert!(audio_asset.load_from_bytes(create_silent_wav()).is_ok());
		assert!(audio_asset.is_loaded());
		sound_system.add_sound_asset("hit", audio_asset);
		sound_system.play_effect(&engine, "hit", 1.0);
		sound_system.play_effect(&engine, "hit", 1.0);
		assert_eq!(sound_system.get_effect_voice_count(), 1);
		for _ in 0..4 {
			sound_system.update(0.11);
			sound_system.play_effect(&engine, "hit", 1.0);
		}
		assert!(sound_system.get_effect_voice_count() <= 2);
	}
}
